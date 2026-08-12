//! Integration tests that exercise the real protocol code against a real
//! server, plus one hermetic mock-socket test that needs no server at all.
//!
//! Everything that talks to an external server is `#[ignore]`d, so a plain
//! `cargo test` stays offline and deterministic. Run them explicitly:
//!
//! ```text
//! WINRMPC_TEST_MPD=10.0.1.3:6600 \
//! WINRMPC_TEST_SNAPCAST=10.0.1.3:1705 \
//!   cargo test -- --ignored --test-threads=1
//! ```
//!
//! The live tests are read-only against the library, and the one that does
//! mutate state (`add_all`) confines itself to a throwaway MPD **partition**
//! it creates and deletes, so the default partition's queue and playback are
//! never touched.

use crate::mpd::types::*;
use crate::mpd::MpdClient;

/// The MPD server to test against, or `None` to skip.
fn mpd_addr() -> Option<String> {
    std::env::var("WINRMPC_TEST_MPD").ok().filter(|s| !s.is_empty())
}

fn snapcast_addr() -> Option<String> {
    std::env::var("WINRMPC_TEST_SNAPCAST").ok().filter(|s| !s.is_empty())
}

/// Connects, and switches into a throwaway partition so queue mutations
/// can't disturb whatever the real `default` partition is doing. Returns the
/// client and the partition name (caller must call `cleanup_partition`).
async fn connect_scratch(addr: &str, name: &str) -> MpdClient {
    let client = MpdClient::new(addr);
    client.connect().await.expect("connect to MPD");
    // Ignore "already exists" — a previous aborted run may have left it.
    let _ = client.new_partition(name).await;
    client
        .switch_partition(name)
        .await
        .expect("switch to scratch partition");
    let status = client.status().await.expect("status");
    assert_eq!(
        status.partition.as_deref(),
        Some(name),
        "must be in the scratch partition before mutating any queue"
    );
    client
}

async fn cleanup_partition(client: &MpdClient, name: &str) {
    let _ = client.clear().await;
    let _ = client.switch_partition("default").await;
    let _ = client.delete_partition(name).await;
}

/// A handful of real song URIs from the server's library, for enqueue tests.
async fn sample_uris(client: &MpdClient, n: usize) -> Vec<String> {
    let albums = client
        .list_albums_by_artist()
        .await
        .expect("list Album group AlbumArtist");
    for (artist, album) in albums.iter() {
        if artist.is_empty() {
            continue;
        }
        let songs = client
            .find_album_by_artist(album, artist)
            .await
            .unwrap_or_default();
        if songs.len() >= n {
            return songs.into_iter().take(n).map(|s| s.file).collect();
        }
    }
    panic!("no album with at least {n} tracks found on the server");
}

// ===========================================================================
// MpdClient::add_all — the command_list bulk enqueue
// ===========================================================================

#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_add_all_enqueues_every_uri_in_one_round_trip() {
    let Some(addr) = mpd_addr() else { return };
    let part = "winrmpc_t1";
    let client = connect_scratch(&addr, part).await;

    let uris = sample_uris(&client, 6).await;
    client.add_all(&uris).await.expect("add_all should succeed");

    let queue = client.queue().await.expect("queue");
    assert_eq!(queue.len(), uris.len(), "every URI should be queued");
    let queued: Vec<String> = queue.iter().map(|s| s.file.clone()).collect();
    assert_eq!(queued, uris, "order must be preserved");

    cleanup_partition(&client, part).await;
}

#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_add_all_empty_input_is_a_no_op() {
    let Some(addr) = mpd_addr() else { return };
    let part = "winrmpc_t2";
    let client = connect_scratch(&addr, part).await;

    client.add_all(&[]).await.expect("empty add_all is Ok");
    assert!(client.queue().await.expect("queue").is_empty());

    cleanup_partition(&client, part).await;
}

/// The documented partial-failure semantics: MPD applies a command list in
/// order and aborts at the first failing command, so tracks *before* the bad
/// URI stay queued and everything after it is dropped. This is the behaviour
/// `PlayAlbum`/`QueueAlbum` now log a warning about instead of swallowing.
#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_add_all_stops_at_the_first_failure_keeping_earlier_tracks() {
    let Some(addr) = mpd_addr() else { return };
    let part = "winrmpc_t3";
    let client = connect_scratch(&addr, part).await;

    let good = sample_uris(&client, 4).await;
    let mut uris = good[..2].to_vec();
    uris.push("winrmpc-does-not-exist/nope.flac".to_string());
    uris.extend_from_slice(&good[2..]);

    let err = client
        .add_all(&uris)
        .await
        .expect_err("a nonexistent URI must surface an error, not be swallowed");

    let queue = client.queue().await.expect("queue");
    assert_eq!(
        queue.len(),
        2,
        "the two tracks before the bad URI stay queued; nothing after it is attempted \
         (got {queue:?}, error was {err})"
    );

    cleanup_partition(&client, part).await;
}

// ===========================================================================
// Album identity against a real library
// ===========================================================================

/// `album_base_and_disc` + `group_albums_by_artist` on the server's actual
/// album list — the multi-disc collapsing this whole area exists for.
#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_album_grouping_collapses_real_multidisc_albums() {
    let Some(addr) = mpd_addr() else { return };
    let client = MpdClient::new(&addr);
    client.connect().await.expect("connect");

    let pairs = client
        .list_albums_by_artist()
        .await
        .expect("list Album group AlbumArtist");
    assert!(!pairs.is_empty(), "server should have albums");

    let groups = group_albums_by_artist(&pairs);
    assert!(
        groups.len() < pairs.len(),
        "grouping must collapse at least some disc variants \
         ({} rows -> {} groups)",
        pairs.len(),
        groups.len()
    );

    // Every collapsed group must have a base name with no disc marker left,
    // and its variants must all strip down to that same base.
    let multi: Vec<&AlbumGroup> = groups.iter().filter(|g| g.variants.len() > 1).collect();
    assert!(
        !multi.is_empty(),
        "this library is known to contain multi-disc sets"
    );
    for g in &multi {
        for v in &g.variants {
            // Case-insensitive: the grouping key folds case, so a variant
            // may legitimately differ from the group's first-seen spelling
            // (one real album is tagged "Decade Of Aggression - Disc 2"
            // alongside "Decade of Aggression - Disc 1 of 2").
            assert_eq!(
                album_base_and_disc(v).0.to_lowercase(),
                g.base.to_lowercase(),
                "variant {v:?} of group {:?} must strip to the group's base",
                g.base
            );
        }
        assert_eq!(
            album_base_and_disc(&g.base).0,
            g.base,
            "a group's base name must itself be marker-free"
        );
    }

    // Art keys: every disc of a set must resolve to one shared cache key.
    // Compared case-insensitively for the same reason as above — the art
    // key is built from the raw tag, which may vary in case across discs.
    for g in &multi {
        let keys: std::collections::HashSet<String> = g
            .variants
            .iter()
            .map(|v| art_key_for(&g.artist, v).to_lowercase())
            .collect();
        assert_eq!(
            keys.len(),
            1,
            "all discs of {:?} must share one art key, got {keys:?}",
            g.base
        );
    }
}

/// Real compilations: where `AlbumArtist` differs from `Artist`, the art key
/// must come from the *album* artist. This is the invariant the Recently
/// Played history was violating by recording only `display_artist()`.
#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_compilation_art_key_uses_album_artist_not_track_artist() {
    let Some(addr) = mpd_addr() else { return };
    let client = MpdClient::new(&addr);
    client.connect().await.expect("connect");

    let pairs = client.list_albums_by_artist().await.expect("albums");
    let mut checked = 0;
    for (artist, album) in pairs.iter() {
        if artist.is_empty() {
            continue;
        }
        let songs = client
            .find_album_by_artist(album, artist)
            .await
            .unwrap_or_default();
        // Find a track whose own artist tag differs from the album artist.
        let Some(track) = songs
            .iter()
            .find(|s| s.display_artist() != s.display_album_artist())
        else {
            continue;
        };

        // What the history used to key on (track artist) vs. what art is
        // actually cached under (Song::art_key(), i.e. album artist).
        let track_artist_key = art_key_for(track.display_artist(), track.display_album());
        assert_ne!(
            track_artist_key,
            track.art_key(),
            "precondition: this track's two artists differ"
        );

        // The fixed path: record album_artist and key off art_artist().
        let entry = RecentlyPlayedEntry {
            file: track.file.clone(),
            title: track.display_title().to_string(),
            artist: track.display_artist().to_string(),
            album_artist: track.display_album_artist().to_string(),
            album: track.display_album().to_string(),
            played_at: 0,
        };
        let groups = recently_played_albums(std::slice::from_ref(&entry));
        assert_eq!(groups.len(), 1);
        assert_eq!(
            art_key_for(&groups[0].artist, &groups[0].album),
            track.art_key(),
            "history tile key must match the key art is cached under"
        );

        checked += 1;
        if checked >= 3 {
            return;
        }
    }
    assert!(
        checked > 0,
        "expected at least one album with a differing track/album artist"
    );
}

// ===========================================================================
// Snapcast
// ===========================================================================

#[tokio::test]
#[ignore = "needs a live Snapcast server; set WINRMPC_TEST_SNAPCAST"]
async fn live_snapcast_get_status_decodes_a_real_server() {
    let Some(addr) = snapcast_addr() else { return };
    let client = crate::snapcast::SnapcastClient::new(&addr);
    client.connect().await.expect("connect to snapserver");
    assert!(client.is_connected().await);

    let (groups, streams) = client.get_status().await.expect("Server.GetStatus");
    assert!(!groups.is_empty(), "real server should report groups");
    assert!(!streams.is_empty(), "real server should report streams");

    for g in &groups {
        assert!(!g.id.is_empty());
        // display_name() falls back to stream_id, so it must never be empty
        // even though real groups routinely have an empty `name`.
        assert!(!g.display_name().is_empty(), "group {:?} has no label", g.id);
        for c in &g.clients {
            assert!(!c.id.is_empty());
            assert!(!c.display_name().is_empty());
            assert!(c.volume <= 100);
        }
    }

    // A second call over the same connection must work (no desync from the
    // response/notification skipping in `SnapcastConnection::request`).
    let (groups2, _) = client.get_status().await.expect("second GetStatus");
    assert_eq!(groups.len(), groups2.len());
}

/// Regression guard for the review finding: a dead socket must be dropped so
/// `is_connected()` reports false and the next call reconnects. Hermetic —
/// it drives a local listener that serves exactly one request then hangs up,
/// so it needs no external server and runs in the normal test suite.
#[tokio::test]
async fn snapcast_drops_a_dead_connection_so_it_can_reconnect() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();

    // Serve one Server.GetStatus per accepted connection, then close.
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            tokio::spawn(async move {
                let (rh, mut wh) = tokio::io::split(stream);
                let mut reader = BufReader::new(rh);
                let mut line = String::new();
                if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                    return;
                }
                let id = serde_json::from_str::<serde_json::Value>(line.trim())
                    .ok()
                    .and_then(|v| v.get("id").and_then(|i| i.as_u64()))
                    .unwrap_or(1);
                let resp = serde_json::json!({
                    "id": id,
                    "jsonrpc": "2.0",
                    "result": { "server": { "groups": [], "streams": [] } },
                });
                let _ = wh.write_all(format!("{resp}\n").as_bytes()).await;
                let _ = wh.flush().await;
                // Then drop both halves: the socket closes, so the client's
                // *next* request hits EOF.
            });
        }
    });

    let client = crate::snapcast::SnapcastClient::new(&addr);
    client.connect().await.expect("initial connect");
    assert!(client.is_connected().await);

    client.get_status().await.expect("first call succeeds");

    // Second call on the now-closed socket must fail...
    let err = client.get_status().await;
    assert!(err.is_err(), "call on a closed socket must error");

    // ...and crucially must have dropped the connection, so the
    // `if !is_connected() { connect() }` guard in on_view_enter fires again.
    // Before the fix this stayed true forever and the view never recovered.
    assert!(
        !client.is_connected().await,
        "a transport error must clear the dead connection"
    );

    // And reconnecting works, which is the actual user-visible recovery.
    client.connect().await.expect("reconnect after drop");
    client.get_status().await.expect("works again after reconnect");
}

/// A JSON-RPC *error response* is a healthy connection reporting a logical
/// error — it must NOT tear the connection down.
#[tokio::test]
async fn snapcast_keeps_the_connection_on_an_rpc_error_response() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();

    tokio::spawn(async move {
        let Ok((stream, _)) = listener.accept().await else { return };
        let (rh, mut wh) = tokio::io::split(stream);
        let mut reader = BufReader::new(rh);
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                return;
            }
            let id = serde_json::from_str::<serde_json::Value>(line.trim())
                .ok()
                .and_then(|v| v.get("id").and_then(|i| i.as_u64()))
                .unwrap_or(1);
            let resp = serde_json::json!({
                "id": id,
                "jsonrpc": "2.0",
                "error": { "code": -32601, "message": "Method not found" },
            });
            if wh.write_all(format!("{resp}\n").as_bytes()).await.is_err() {
                return;
            }
            let _ = wh.flush().await;
        }
    });

    let client = crate::snapcast::SnapcastClient::new(&addr);
    client.connect().await.expect("connect");
    assert!(client.get_status().await.is_err(), "server returns an RPC error");
    assert!(
        client.is_connected().await,
        "an RPC error response proves the socket is fine — don't drop it"
    );
}




/// Guards the `replay_gain_status` / `replay_gain_mode` spelling. Both were
/// previously sent without the underscore between "replay" and "gain", which
/// MPD rejects outright with `ACK [5@0] {} unknown command`, so replay gain
/// silently never worked. A unit test can only check the string we build;
/// only a live server proves MPD accepts it.
#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_replay_gain_round_trips() {
    let Some(addr) = mpd_addr() else { return };
    let client = MpdClient::new(&addr);
    client.connect().await.expect("connect");

    let original = client
        .replay_gain_status()
        .await
        .expect("replay_gain_status must be a real command");
    assert!(
        ["off", "track", "album", "auto"].contains(&original.as_str()),
        "unexpected mode {original:?}"
    );

    for mode in ["track", "album", "auto", "off"] {
        client
            .set_replay_gain_mode(mode)
            .await
            .expect("replay_gain_mode must be a real command");
        assert_eq!(client.replay_gain_status().await.expect("status"), mode);
    }

    // Leave the server exactly as we found it.
    client.set_replay_gain_mode(&original).await.expect("restore");
    assert_eq!(client.replay_gain_status().await.expect("status"), original);
}

/// Reproduces the album-grid desync: many concurrent art fetches sharing one
/// connection, interleaved with the 500ms status poll. Symptom in the field
/// was every subsequent command failing with "stream did not contain valid
/// UTF-8" and, damningly, `currentsong` receiving an ACK addressed to
/// `{albumart}` — i.e. an unread response left in the socket.
#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_concurrent_art_fetches_do_not_desync_the_connection() {
    let Some(addr) = mpd_addr() else { return };
    let client = MpdClient::new(&addr);
    client.connect().await.expect("connect");

    // A spread of real albums, like a grid page.
    let pairs = client.list_albums_by_artist().await.expect("albums");
    let mut uris = Vec::new();
    for (artist, album) in pairs.iter().take(40) {
        if artist.is_empty() { continue; }
        if let Ok(songs) = client.find_album_by_artist(album, artist).await {
            if let Some(s) = songs.first() {
                uris.push(s.file.clone());
            }
        }
        if uris.len() >= 12 { break; }
    }
    assert!(uris.len() >= 4, "need a few albums to hammer");

    // Same shape as App::prefetch_album_art: bounded concurrency over the
    // one shared connection, plus the status poll running alongside.
    let gate = std::sync::Arc::new(tokio::sync::Semaphore::new(4));
    let mut handles = Vec::new();
    for uri in uris.clone() {
        let c = client.clone();
        let g = gate.clone();
        handles.push(tokio::spawn(async move {
            let _p = g.acquire().await;
            let _ = c.tag_art(&uri).await;
            let _ = c.cover_file_art(&uri).await;
        }));
    }
    let poller = {
        let c = client.clone();
        tokio::spawn(async move {
            for _ in 0..40 {
                let _ = c.status().await;
                let _ = c.current_song().await;
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
    };
    for h in handles { let _ = h.await; }
    let _ = poller.await;

    // The connection must still be usable and correctly framed.
    let status = client
        .status()
        .await
        .expect("status must still work after concurrent art fetches");
    let _ = status.state;
    let stats = client.stats().await.expect("stats must still work");
    assert!(stats.songs > 0, "a framed response should carry real data");
}

/// Diagnostic for "this album shows no cover in the grid".
///
/// Read-only. For each album named below it runs exactly what
/// `App::fetch_album_art_local` runs — `find Album <tag>`, then `readpicture`
/// and `albumart` on the first track — and prints which stage answered. That
/// separates the two explanations a blank tile has: the library genuinely
/// holds no local art for it (so only MusicBrainz can help), or the lookup
/// itself is failing (wrong tag, escaping, no tracks found).
///
/// Not an assertion of coverage — it asserts only that every album resolves
/// to at least one track, which is the part that would be *our* bug.
#[tokio::test]
#[ignore]
async fn live_diagnose_album_art_sources() {
    let Some(addr) = mpd_addr() else { return };
    let client = MpdClient::new(&addr);
    client.connect().await.expect("connect to MPD");

    // Albums reported as showing no cover, plus a few known-good controls.
    let albums = [
        "'74 Jailbreak",
        "Powerage",
        "Pyramid",
        "Jackie Brown",
        "A Long Day's Night (Live)",
        "Fire Of Unknown Origin",
        "Curse Of The Hidden Mirror",
        "Greatest Hits",
        "A Broken Frame [UK]",
        "Speak & Spell [UK]",
        "The Singles 81>85",
        "Songs of Faith and Devotion",
        "Sounds of the Universe",
        "Like An Ever Flowing Stream",
        "Dream Evil",
        "Le som en fotomodell",
        // Controls — these do render art in the grid.
        "The Razor's Edge",
        "Master Of Reality",
    ];

    let mut no_tracks = Vec::new();
    for album in albums {
        let songs = client.find("Album", album).await.unwrap_or_default();
        let Some(first) = songs.first() else {
            println!("{album:40} NO TRACKS FOUND");
            no_tracks.push(album);
            continue;
        };
        let tag = client.tag_art(&first.file).await;
        let cover = client.cover_file_art(&first.file).await;
        let verdict = match (&tag, &cover) {
            (Ok(Some(d)), _) => format!("readpicture {} KB", d.len() / 1024),
            (_, Ok(Some(d))) => format!("albumart {} KB", d.len() / 1024),
            (Err(e), _) => format!("ERROR readpicture: {e}"),
            (_, Err(e)) => format!("ERROR albumart: {e}"),
            _ => "no local art (MusicBrainz only)".to_string(),
        };
        println!(
            "{album:40} {verdict}\n{:44}artist={:?} albumartist={:?}\n{:44}{}",
            "",
            first.artist.as_deref().unwrap_or("-"),
            first.album_artist.as_deref().unwrap_or("-"),
            "",
            first.file
        );
    }

    assert!(
        no_tracks.is_empty(),
        "these albums resolved to no tracks at all, which is a lookup bug on our side: {no_tracks:?}"
    );
}

/// Probe: can `albumart` find a cover for a CUE-sheet track if we ask about
/// the `.cue` file itself rather than the virtual `…/track0001` inside it?
///
/// MPD derives the directory to search from the URI's parent. For a cue
/// track that parent is the `.cue` file, which isn't a real directory, so the
/// lookup can only fail. Truncating to the `.cue` path makes the parent the
/// actual album folder.
#[tokio::test]
#[ignore]
async fn live_probe_cue_album_art_fallback() {
    let Some(addr) = mpd_addr() else { return };
    let client = MpdClient::new(&addr);
    client.connect().await.expect("connect to MPD");

    for album in ["A Broken Frame [UK]", "Speak & Spell [UK]", "Songs of Faith and Devotion"] {
        let songs = client.find("Album", album).await.unwrap_or_default();
        let Some(first) = songs.first() else { continue };
        let uri = &first.file;
        let Some(cue_end) = uri.to_lowercase().find(".cue/") else {
            println!("{album:34} not a cue track");
            continue;
        };
        let cue_path = &uri[..cue_end + 4];
        let direct = client.cover_file_art(uri).await;
        let via_cue = client.cover_file_art(cue_path).await;
        println!(
            "{album:34} direct={:24} via .cue={}",
            match &direct {
                Ok(Some(d)) => format!("{} KB", d.len() / 1024),
                Ok(None) => "none".into(),
                Err(_) => "ACK".into(),
            },
            match &via_cue {
                Ok(Some(d)) => format!("{} KB  <-- WORKS", d.len() / 1024),
                Ok(None) => "none".into(),
                Err(e) => format!("ACK ({e})"),
            }
        );
    }
}

/// End-to-end check of the *external* lookup path for albums this library
/// has no local art for. Hits MusicBrainz and the Cover Art Archive for
/// real, so it needs its own opt-in (`WINRMPC_TEST_MUSICBRAINZ=1`) on top of
/// `#[ignore]` — a plain `--ignored` sweep must not start hammering a free
/// community service.
///
/// The artist strings are the exact tags from this library, mojibake and
/// double spaces included: they are the input the matching rules have to
/// cope with. Uses a throwaway cache dir so a previous run's negatives
/// can't make a broken lookup look fixed.
#[tokio::test]
#[ignore]
async fn live_musicbrainz_resolves_locally_artless_albums() {
    if std::env::var("WINRMPC_TEST_MUSICBRAINZ").ok().as_deref() != Some("1") {
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "winrmpc-mb-livetest-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).ok();
    let store = crate::store::Store::open(&dir);
    let mb = crate::art::MusicBrainzClient::new(store);

    // (album tag, artist tag) exactly as they appear in the library.
    let cases = [
        ("'74 Jailbreak", "ACDC"),
        ("Powerage", "ACDC"),
        ("Pyramid", "Alan Parsons Project, The"),
        ("A Long Day's Night (Live)", "Blue Oyster Cult"),
        ("Fire Of Unknown Origin", "Blue  Oyster Cult"),
        ("Curse Of The Hidden Mirror", "Blue Îyster Cult"),
        ("A Broken Frame [UK]", "Depeche Mode"),
        ("Speak & Spell [UK]", "Depeche Mode"),
        ("The Singles 81>85", "Depeche Mode"),
        ("Songs of Faith and Devotion", "Depeche Mode"),
        ("Sounds of the Universe", "Depeche Mode"),
        ("Dream Evil", "Dio"),
        ("Master Of Reality", "Black Sabbath"),
        ("Jackie Brown", ""),
    ];

    let mut missing = Vec::new();
    for (album, artist) in cases {
        match mb.fetch_album_art(artist, album).await {
            Some(data) => println!("  OK    {album:32} [{artist}] {} KB", data.len() / 1024),
            None => {
                println!("  MISS  {album:32} [{artist}]");
                missing.push(album);
            }
        }
    }
    println!("\n{} of {} resolved", cases.len() - missing.len(), cases.len());
    if !missing.is_empty() {
        println!("still missing: {missing:?}");
    }
}

/// End-to-end check of the Wikipedia bio path over the same awkward tags the
/// art path is tested against. Same opt-in as the MusicBrainz test
/// (`WINRMPC_TEST_MUSICBRAINZ=1`), since it hits MusicBrainz for the curated
/// URL relation before it ever reaches Wikipedia.
///
/// Prints the first line of each bio so a *wrong* match is visible — the
/// failure mode here isn't an empty result, it's confidently returning the
/// article for somebody else's identically-titled record.
#[tokio::test]
#[ignore]
async fn live_wikipedia_bios_for_awkward_tags() {
    if std::env::var("WINRMPC_TEST_MUSICBRAINZ").ok().as_deref() != Some("1") {
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "winrmpc-wiki-livetest-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).ok();
    let store = crate::store::Store::open(&dir);
    let mb = crate::art::MusicBrainzClient::new(store);

    println!("\n-- artists --");
    for artist in [
        "ACDC",
        "Alan Parsons Project, The",
        "Blue  Oyster Cult",
        "Motorhead",
        "Depeche Mode",
    ] {
        match mb.fetch_artist_bio(artist).await {
            Some(bio) => println!("  OK    {artist:28} {}", first_sentence(&bio)),
            None => println!("  MISS  {artist:28}"),
        }
    }

    println!("\n-- albums --");
    for (album, artist) in [
        ("A Broken Frame [UK]", "Depeche Mode"),
        ("Powerage", "ACDC"),
        ("Fire Of Unknown Origin", "Blue  Oyster Cult"),
        ("Pyramid", "Alan Parsons Project, The"),
        // The generic-title guard: this must not return a generic
        // "Greatest Hits" article, and must not attribute someone else's.
        ("Greatest Hits", "Bob Dylan"),
    ] {
        match mb.fetch_album_bio(artist, album).await {
            Some(bio) => println!("  OK    {album:28} [{artist}] {}", first_sentence(&bio)),
            None => println!("  MISS  {album:28} [{artist}]"),
        }
    }
}

fn first_sentence(s: &str) -> String {
    let cut = s.find(". ").map(|i| i + 1).unwrap_or(s.len().min(150));
    s[..cut.min(s.len()).min(180)].replace('\n', " ")
}
