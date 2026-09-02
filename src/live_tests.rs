//! Integration tests that exercise the real protocol code against a real
//! server, plus one hermetic mock-socket test that needs no server at all.
//!
//! Everything that talks to an external server is `#[ignore]`d, so a plain
//! `cargo test` stays offline and deterministic. Run them explicitly:
//!
//! ```text
//! WINRMPC_TEST_MPD=10.0.1.3:6600 \
//! WINRMPC_TEST_MPD_PASSWORD=secret \      # only if the server wants one
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

/// The password for that server, if it wants one. Without this the suite
/// cannot run against a password-protected server at all — every command
/// comes back `ACK [4@0] … you don't have permission`.
fn mpd_password() -> Option<String> {
    std::env::var("WINRMPC_TEST_MPD_PASSWORD").ok().filter(|s| !s.is_empty())
}

fn snapcast_addr() -> Option<String> {
    std::env::var("WINRMPC_TEST_SNAPCAST").ok().filter(|s| !s.is_empty())
}

/// Connects, and switches into a throwaway partition so queue mutations
/// can't disturb whatever the real `default` partition is doing. Returns the
/// client and the partition name (caller must call `cleanup_partition`).
async fn connect_scratch(addr: &str, name: &str) -> MpdClient {
    let client = MpdClient::new(addr, mpd_password());
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
    let client = MpdClient::new(&addr, mpd_password());
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
            // Compared through `album_grouping_key`, not by string equality:
            // a variant may legitimately differ from the group's first-seen
            // spelling in case (this library has "Decade Of Aggression -
            // Disc 2" beside "Decade of Aggression - Disc 1 of 2") *and* in
            // punctuation (The Beatles' "1967-1970" beside "1967–1970",
            // hyphen against en dash — the case the folding was added for).
            // Asserting raw equality here would be asserting that the
            // folding doesn't work.
            assert_eq!(
                album_grouping_key(&g.artist, &album_base_and_disc(v).0),
                album_grouping_key(&g.artist, &g.base),
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
    //
    // Asserted the way the **app** builds it — from the group's `base`, which
    // is what `album_art_targets`, the grid tiles and `enqueue_album_art` all
    // use. That is the path a blank cover would actually come from.
    for g in &multi {
        let keys: std::collections::HashSet<String> = g
            .variants
            .iter()
            .map(|_| art_key_for(&g.artist, &g.base))
            .collect();
        assert_eq!(
            keys.len(),
            1,
            "all discs of {:?} must share one art key, got {keys:?}",
            g.base
        );
    }

    // Diagnostic, not an assertion. `art_key_for` disc-strips but does *not*
    // fold case or punctuation, so a per-track key built from the raw tag
    // (`Song::art_key()`, which is what Now Playing uses) can differ from the
    // group-level key the grid stores under — the cover then shows in one
    // place and not the other. Pre-existing: it already split on case before
    // the punctuation folding made it visible here. Printed rather than
    // failed because folding the art key would re-key every cached cover in
    // every existing install; see docs/status.md.
    let mut split = 0usize;
    for g in &multi {
        let raw: std::collections::HashSet<String> = g
            .variants
            .iter()
            .map(|v| art_key_for(&g.artist, v))
            .collect();
        if raw.len() > 1 {
            split += 1;
            eprintln!("art key splits across discs: {:?} -> {raw:?}", g.base);
        }
    }
    eprintln!("{split} of {} multi-disc groups split their art key", multi.len());
}

/// Real compilations: where `AlbumArtist` differs from `Artist`, the art key
/// must come from the *album* artist. This is the invariant the Recently
/// Played history was violating by recording only `display_artist()`.
#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_compilation_art_key_uses_album_artist_not_track_artist() {
    let Some(addr) = mpd_addr() else { return };
    let client = MpdClient::new(&addr, mpd_password());
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
    let client = MpdClient::new(&addr, mpd_password());
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
    let client = MpdClient::new(&addr, mpd_password());
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
    let client = MpdClient::new(&addr, mpd_password());
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
    let client = MpdClient::new(&addr, mpd_password());
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

/// Every partition's output list must come back free of MPD's `dummy`
/// placeholders.
///
/// This is not a rare post-move artifact — it is the **steady state** of any
/// multi-partition setup. On the development server the `default` partition
/// reports ten outputs of which **nine are dummies**, one for every output
/// that actually lives in another partition. Unfiltered, the Outputs view is
/// mostly ghosts that look exactly like the real thing and do nothing when
/// enabled.
///
/// Read-only: it switches the *connection's* partition, which is per-session
/// state, and restores it. Nothing on the server is modified.
#[tokio::test]
#[ignore]
async fn live_outputs_never_include_dummy_placeholders() {
    let Some(addr) = mpd_addr() else {
        eprintln!("skipping: set WINRMPC_TEST_MPD");
        return;
    };
    let client = MpdClient::new(&addr, mpd_password());
    client.connect().await.expect("connect");

    let original = client
        .status()
        .await
        .ok()
        .and_then(|s| s.partition)
        .unwrap_or_else(|| "default".into());

    let partitions = client.list_partitions().await.expect("listpartitions");
    assert!(!partitions.is_empty(), "server reports no partitions");

    let mut total = 0usize;
    for p in &partitions {
        client.switch_partition(&p.name).await.expect("partition");
        let outs = client.outputs().await.expect("outputs");
        for o in &outs {
            assert!(
                !o.plugin.eq_ignore_ascii_case(crate::mpd::commands::DUMMY_PLUGIN),
                "partition {} still lists a dummy placeholder: {} (id {})",
                p.name,
                o.name,
                o.id
            );
            assert!(
                !o.plugin.is_empty(),
                "partition {} output {} has no plugin at all",
                p.name,
                o.name
            );
        }
        println!("  {:<12} {} real output(s)", p.name, outs.len());
        total += outs.len();
    }
    client.switch_partition(&original).await.ok();

    assert!(total > 0, "no real outputs anywhere — the filter is too greedy");
}

/// Fetches real synced lyrics from LRCLIB and checks they parse into
/// timestamped lines that actually advance.
///
/// There was no live test for lyrics at all, which left `parse_lrc` verified
/// only against fixtures this repo wrote itself — so nothing confirmed that
/// what LRCLIB *actually returns* matches the shape the parser expects. Needs
/// no MPD server. Same `WINRMPC_TEST_NETWORK=1` gate as the TLS check.
#[tokio::test]
#[ignore]
async fn live_lrclib_returns_parseable_synced_lyrics() {
    if std::env::var("WINRMPC_TEST_NETWORK").ok().as_deref() != Some("1") {
        eprintln!("skipping: set WINRMPC_TEST_NETWORK=1 to run");
        return;
    }

    let client = crate::lyrics::LyricsClient::new();

    // Well-known tracks with good LRCLIB coverage. Durations are the real
    // ones — the exact-match endpoint uses them to pick the right recording.
    let tracks = [
        ("Radiohead", "Creep", "Pablo Honey", Some(238.0)),
        ("Nirvana", "Smells Like Teen Spirit", "Nevermind", Some(301.0)),
        ("a-ha", "Take On Me", "Hunting High and Low", Some(225.0)),
    ];

    let mut synced_found = 0;
    for (artist, title, album, dur) in tracks {
        match client.fetch(artist, title, album, dur).await {
            Some(l) => {
                let plain = l.plain.as_ref().map(|p| p.lines().count()).unwrap_or(0);
                match &l.synced {
                    Some(lines) => {
                        synced_found += 1;
                        println!(
                            "  OK    {title:26} synced={} lines, plain={plain} lines, \
                             first=[{:.2}s] {:?}, last=[{:.2}s]",
                            lines.len(),
                            lines[0].secs,
                            lines.iter().find(|l| !l.text.is_empty()).map(|l| &l.text),
                            lines[lines.len() - 1].secs,
                        );

                        // The properties the Now Playing panel relies on.
                        assert!(lines.len() > 5, "{title}: implausibly few synced lines");
                        assert!(
                            lines.windows(2).all(|w| w[0].secs <= w[1].secs),
                            "{title}: synced lines must be sorted by time — \
                             the active-line search uses rposition, which assumes it"
                        );
                        assert!(
                            lines.iter().any(|l| !l.text.is_empty()),
                            "{title}: every synced line is empty, so the parser \
                             kept the timestamps and dropped the words"
                        );
                        assert!(
                            lines[lines.len() - 1].secs > lines[0].secs,
                            "{title}: timestamps never advance"
                        );
                        // A timestamp past the track's own length means the
                        // parser mis-scaled something (mm vs ss, say).
                        if let Some(d) = dur {
                            assert!(
                                lines[lines.len() - 1].secs < d + 60.0,
                                "{title}: last timestamp {:.1}s is beyond the track \
                                 duration {d}s",
                                lines[lines.len() - 1].secs
                            );
                        }
                    }
                    None => println!("  PLAIN {title:26} no synced lyrics (plain={plain} lines)"),
                }
            }
            None => println!("  MISS  {title:26} nothing found"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    }

    assert!(
        synced_found > 0,
        "no synced lyrics came back for any track — either LRCLIB changed its \
         response shape or parse_lrc is dropping every line"
    );
}

/// Proves the TLS stack actually negotiates against every host this app talks
/// to, on whatever OS the test is run on.
///
/// This is the acceptance check for the rustls switch
/// (docs/plans/network-fetch-cross-platform.md, finding 1): the unit tests can
/// only assert that a client *builds*, and building is not the part that
/// breaks when a TLS backend or a root store is wrong. Nothing here needs MPD,
/// so it is the one live test that runs on a machine with no music server —
/// which also makes it the fastest way to tell "the network is broken" apart
/// from "the lookup logic is broken".
///
/// Gated behind `WINRMPC_TEST_NETWORK=1` rather than the MusicBrainz flag: it
/// issues one cheap request per host and does no searching, so it is not the
/// kind of load that flag exists to hold back.
///
/// ```text
/// WINRMPC_TEST_NETWORK=1 cargo test -- --ignored live_tls_reaches_every_lookup_host
/// ```
#[tokio::test]
#[ignore]
async fn live_tls_reaches_every_lookup_host() {
    if std::env::var("WINRMPC_TEST_NETWORK").ok().as_deref() != Some("1") {
        eprintln!("skipping: set WINRMPC_TEST_NETWORK=1 to run");
        return;
    }

    let client = crate::net::client("live TLS check").expect("HTTP client should build");

    // One representative endpoint per host in the fetch path. Cover Art
    // Archive is included specifically because it 307s to archive.org, so a
    // success here also proves redirect following survives the TLS change.
    let targets = [
        ("MusicBrainz", "https://musicbrainz.org/ws/2/artist?query=test&limit=1&fmt=json"),
        ("Cover Art Archive", "https://coverartarchive.org/release-group/f5093c06-23e3-404f-aeaa-40f72885ee3a"),
        ("Wikipedia", "https://en.wikipedia.org/api/rest_v1/page/summary/Music"),
        ("LRCLIB", "https://lrclib.net/api/search?artist_name=test&track_name=test"),
    ];

    let mut failures = Vec::new();
    for (name, url) in targets {
        match client.get(url).send().await {
            Ok(resp) => println!("  OK    {name:18} HTTP {}", resp.status()),
            Err(e) => {
                println!("  FAIL  {name:18} {e}");
                failures.push(format!("{name}: {e}"));
            }
        }
        // Be polite even in a connectivity check.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    }

    assert!(
        failures.is_empty(),
        "TLS/transport failed for: {failures:?}"
    );
}

// ============================================================================
// Recently Added
// ============================================================================

/// The bug this guards, and the reason it has to be a *live* test: MPD
/// returns `find` matches in database order (documented as undefined —
/// effectively directory traversal), and `window` slices **that**. The old
/// query had no `sort`, so once the library held more matches than the
/// window, which songs came back was decided by path order and the newest
/// additions were silently dropped. Sorting client-side afterwards can't
/// recover a row the server never sent.
///
/// A unit test can only check the string we build. Only a real server proves
/// the server honours it, so the window here is deliberately **smaller** than
/// the match count — that's the exact condition under which the old query
/// returns an arbitrary slice.
#[tokio::test]
#[ignore]
async fn live_recently_added_is_newest_first() {
    let Some(addr) = mpd_addr() else {
        eprintln!("skipping: set WINRMPC_TEST_MPD");
        return;
    };
    let client = MpdClient::new(&addr, mpd_password());
    client.connect().await.expect("connect to MPD");

    // Ten years back, so a library of any age has far more matches than the
    // window below.
    let since = (chrono::Utc::now() - chrono::Duration::days(3650))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    let (songs, rung) = client
        .find_recently_added(&since, 50)
        .await
        .expect("recently-added query");
    eprintln!("rung: {rung:?}, {} songs", songs.len());

    assert!(!songs.is_empty(), "library has no songs at all?");
    if !rung.is_server_sorted() {
        eprintln!("server has no `sort` clause — ordering is not guaranteed, skipping");
        return;
    }

    // Assert on the field the *rung* sorted by. Asserting on `last_modified`
    // regardless is how this test first failed against a 0.24 server: the top
    // rung sorts by `Added`, and on this library mtime is uniform (a mass
    // rewrite) while `Added` spans months, so the two orders have nothing to
    // do with each other.
    fn key(s: &Song, rung: RecentlyAddedRung) -> Option<&String> {
        match rung {
            RecentlyAddedRung::AddedSince => s.added.as_ref(),
            _ => s.last_modified.as_ref(),
        }
    }
    // Descending, with unknown timestamps allowed only at the tail.
    let stamps: Vec<Option<&String>> = songs.iter().map(|s| key(s, rung)).collect();
    for pair in stamps.windows(2) {
        match (pair[0], pair[1]) {
            (Some(a), Some(b)) => assert!(
                a >= b,
                "not newest-first: {a} came before {b} — the server ignored `sort`"
            ),
            (None, Some(b)) => panic!("a song with no timestamp sorted above {b}"),
            _ => {}
        }
    }

    // And the slice really is the newest end of the library, not a slice of
    // path order: a second, larger window must not surface anything newer.
    let (wider, _) = client
        .find_recently_added(&since, 500)
        .await
        .expect("wider recently-added query");
    let newest_narrow = stamps.first().copied().flatten();
    let newest_wide = wider.iter().filter_map(|s| key(s, rung)).max();
    assert_eq!(
        newest_narrow, newest_wide,
        "a wider window found a newer song, so the narrow one was not the newest end"
    );
}

/// Records which rung this server actually supports. Not an assertion about
/// the server — a 0.21 box legitimately answers on the bottom rung — but the
/// ladder is invisible from the outside otherwise, and knowing which one
/// answers is what tells you whether Recently Added is using real add-times
/// or file mtimes.
#[tokio::test]
#[ignore]
async fn live_recently_added_reports_which_rung_the_server_answers_on() {
    let Some(addr) = mpd_addr() else {
        eprintln!("skipping: set WINRMPC_TEST_MPD");
        return;
    };
    let client = MpdClient::new(&addr, mpd_password());
    client.connect().await.expect("connect to MPD");

    let since = (chrono::Utc::now() - chrono::Duration::days(3650))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    // A fresh client starts at the top of the ladder, so whichever rung it
    // settles on is the highest this server supports.
    let (_, chosen) = client
        .find_recently_added(&since, 1)
        .await
        .expect("some rung must answer");
    eprintln!("highest supported rung: {chosen:?}");
    if !chosen.is_server_sorted() {
        eprintln!(
            "NOTE: no `sort` clause — Recently Added on this server shows an arbitrary \
             slice, not the newest additions"
        );
    }

    // And it must remember it: the second call issues no further probing.
    let (_, again) = client
        .find_recently_added(&since, 1)
        .await
        .expect("second call");
    assert_eq!(chosen, again, "the probed rung was not cached");
}

/// A mock MPD server that answers `find` only when the command contains none
/// of `reject`, and ACKs otherwise. Records every command it was sent.
///
/// This is how the pre-0.24 / pre-0.22 fallbacks get tested at all: they can
/// only be exercised against a server that *lacks* the newer syntax, and the
/// one real server available runs 0.24. A mock is the difference between
/// "the fallback is written" and "the fallback works".
#[cfg(test)]
async fn mock_mpd_rejecting(
    version: &'static str,
    reject: &'static [&'static str],
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();

    let recorder = std::sync::Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            let recorder = std::sync::Arc::clone(&recorder);
            tokio::spawn(async move {
                let (rh, mut wh) = tokio::io::split(stream);
                let mut reader = BufReader::new(rh);
                let _ = wh.write_all(format!("OK MPD {version}\n").as_bytes()).await;
                let _ = wh.flush().await;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let cmd = line.trim_end().to_string();
                    recorder.lock().unwrap().push(cmd.clone());

                    let reply = if reject.iter().any(|r| cmd.contains(r)) {
                        // What a real MPD says when it doesn't know a filter
                        // or sort name.
                        "ACK [2@0] {find} Unknown filter type\n".to_string()
                    } else {
                        "file: a/b.flac\nLast-Modified: 2026-08-01T00:00:00Z\nOK\n".to_string()
                    };
                    let _ = wh.write_all(reply.as_bytes()).await;
                    let _ = wh.flush().await;
                }
            });
        }
    });

    (addr, seen)
}

/// MPD 0.23: no `added-since`, but `sort` works. Must land on the middle
/// rung — mtime, still sorted server-side, so the window still keeps the
/// newest end.
#[tokio::test]
async fn recently_added_falls_back_to_modified_since_on_a_pre_0_24_server() {
    let (addr, seen) = mock_mpd_rejecting("0.23.5", &["added-since"]).await;
    // Hermetic mock: no auth, and deliberately no dependence on the env var.
    let client = MpdClient::new(&addr, None);
    client.connect().await.expect("connect");

    let (songs, rung) = client
        .find_recently_added("2026-01-01T00:00:00Z", 10)
        .await
        .expect("must degrade rather than fail");

    assert_eq!(rung, RecentlyAddedRung::ModifiedSinceSorted);
    assert!(rung.is_server_sorted(), "0.23 has sort; don't give it up too");
    assert_eq!(songs.len(), 1, "the fallback query's result must be parsed");

    let sent = seen.lock().unwrap().clone();
    assert!(sent.iter().any(|c| c.contains("added-since")), "never probed");
    assert!(sent.iter().any(|c| c.contains("sort -Last-Modified")));
    assert!(
        !sent.iter().any(|c| c.contains("sort -Added")
            && !c.contains("added-since")),
        "the `Added` sort name must not leak onto the mtime query"
    );
}

/// MPD 0.21: neither `added-since` nor `sort`. Must reach the bottom rung and
/// report itself as unsorted, which is what makes the app warn that the list
/// is an arbitrary slice rather than the newest additions.
#[tokio::test]
async fn recently_added_falls_back_to_the_legacy_query_on_a_pre_0_22_server() {
    let (addr, seen) = mock_mpd_rejecting("0.21.0", &["added-since", "sort"]).await;
    // Hermetic mock: no auth, and deliberately no dependence on the env var.
    let client = MpdClient::new(&addr, None);
    client.connect().await.expect("connect");

    let (songs, rung) = client
        .find_recently_added("2026-01-01T00:00:00Z", 10)
        .await
        .expect("an ancient server must still show something");

    assert_eq!(rung, RecentlyAddedRung::ModifiedSinceUnsorted);
    assert!(!rung.is_server_sorted());
    assert_eq!(songs.len(), 1);

    let sent = seen.lock().unwrap().clone();
    assert_eq!(sent.len(), 3, "all three rungs should have been tried once");
}

/// The probe must happen once, not on every view entry. Three ACKs per visit
/// to Recently Added is three wasted round trips on exactly the servers least
/// able to spare them.
#[tokio::test]
async fn the_recently_added_rung_is_probed_once_and_then_remembered() {
    let (addr, seen) = mock_mpd_rejecting("0.21.0", &["added-since", "sort"]).await;
    // Hermetic mock: no auth, and deliberately no dependence on the env var.
    let client = MpdClient::new(&addr, None);
    client.connect().await.expect("connect");

    for _ in 0..3 {
        client
            .find_recently_added("2026-01-01T00:00:00Z", 10)
            .await
            .expect("query");
    }

    let sent = seen.lock().unwrap().clone();
    // 3 for the first call's descent, then 1 each for the two after it.
    assert_eq!(sent.len(), 5, "sent: {sent:#?}");
    assert_eq!(
        sent.iter().filter(|c| c.contains("added-since")).count(),
        1,
        "the top rung must not be re-probed"
    );
}

/// A clone shares the cache — `App` clones the client into every task, so a
/// per-clone cache would mean re-probing on essentially every call.
#[tokio::test]
async fn a_cloned_client_shares_the_probed_rung() {
    let (addr, seen) = mock_mpd_rejecting("0.21.0", &["added-since", "sort"]).await;
    // Hermetic mock: no auth, and deliberately no dependence on the env var.
    let client = MpdClient::new(&addr, None);
    client.connect().await.expect("connect");
    client
        .find_recently_added("2026-01-01T00:00:00Z", 10)
        .await
        .expect("first");

    let cloned = client.clone();
    let (_, rung) = cloned
        .find_recently_added("2026-01-01T00:00:00Z", 10)
        .await
        .expect("clone");

    assert_eq!(rung, RecentlyAddedRung::ModifiedSinceUnsorted);
    assert_eq!(seen.lock().unwrap().len(), 4, "the clone re-probed");
}

/// The Search view's Artists/Albums sections are derived from the songs one
/// `search any` already returned. This is the check that the derivation
/// survives real tag shapes — this library holds `"Blue  Oyster Cult"` with
/// two spaces, `"Alan Parsons Project, The"` in sort order, and `"ACDC"`
/// without the slash.
///
/// Prints what it found, so it doubles as the diagnostic for "why is this
/// artist missing from search".
#[tokio::test]
#[ignore = "needs a live MPD server; set WINRMPC_TEST_MPD"]
async fn live_search_sections_are_derived_from_real_results() {
    let Some(addr) = mpd_addr() else {
        eprintln!("skipping: set WINRMPC_TEST_MPD");
        return;
    };
    let client = MpdClient::new(&addr, mpd_password());
    client.connect().await.expect("connect");

    for query in ["beatles", "oyster", "greatest hits"] {
        let songs = client.search("any", query).await.expect("search");
        let sections = search_sections(&songs, query);
        eprintln!(
            "{query:>14}: {} songs, {} albums, {} artists {:?}",
            songs.len(),
            sections.albums.len(),
            sections.artists.len(),
            sections.artists,
        );

        // Whatever the library holds, the sections must never invent an
        // entity that isn't in the results, and never offer a placeholder —
        // navigating to one renders an empty page.
        for artist in &sections.artists {
            assert!(artist != "Unknown Artist");
            assert!(
                songs.iter().any(|s| s.display_artist() == artist
                    || s.display_album_artist() == artist),
                "{artist:?} is not in the results it was derived from"
            );
        }
        for album in &sections.albums {
            assert!(album.base != "Unknown Album");
            assert!(!album.variants.is_empty(), "an album row with no variants");
        }
    }
}

/// A mock MPD that actually wants a password: every command is answered with
/// the permission ACK a real server sends until the right `password` line
/// arrives. This is the only way to test authentication offline — the real
/// test server doesn't require one, and a server that does can't be asked to
/// stop.
async fn mock_mpd_requiring_password(
    pw: &'static str,
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();

    let recorder = std::sync::Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            let recorder = std::sync::Arc::clone(&recorder);
            tokio::spawn(async move {
                let (rh, mut wh) = tokio::io::split(stream);
                let mut reader = BufReader::new(rh);
                let _ = wh.write_all(b"OK MPD 0.24.0\n").await;
                let _ = wh.flush().await;
                // Authentication is per *connection*, so this state is
                // deliberately scoped to the socket and not to the mock.
                let mut authed = false;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let cmd = line.trim_end().to_string();
                    recorder.lock().unwrap().push(cmd.clone());

                    let reply = if let Some(rest) = cmd.strip_prefix("password ") {
                        if rest == format!("\"{pw}\"") {
                            authed = true;
                            "OK\n".to_string()
                        } else {
                            "ACK [3@0] {password} incorrect password\n".to_string()
                        }
                    } else if !authed {
                        let verb = cmd.split(' ').next().unwrap_or("");
                        format!("ACK [4@0] {{{verb}}} you don't have permission for \"{verb}\"\n")
                    } else {
                        "OK\n".to_string()
                    };
                    let _ = wh.write_all(reply.as_bytes()).await;
                    let _ = wh.flush().await;
                }
            });
        }
    });

    (addr, seen)
}

/// The password has to go out *inside* `connect`, before anything else is
/// sent. This is the whole bug: `MpdClient::password` existed and nothing
/// ever called it, so a password-protected server was met with an
/// unauthenticated connection and every command came back with a permission
/// ACK.
#[tokio::test]
async fn connect_authenticates_before_it_sends_anything_else() {
    let (addr, seen) = mock_mpd_requiring_password("s3cret").await;
    let client = MpdClient::new(&addr, Some("s3cret".into()));

    client.connect().await.expect("connect with the right password");
    client.status().await.expect("a command after authenticating");

    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.first().map(String::as_str),
        Some("password \"s3cret\""),
        "the very first line on the socket must be the password, not a command \
         that the server will refuse"
    );
    assert!(seen.iter().any(|c| c == "status"));
}

/// A wrong password must fail the *connect*, not merely log a failed command.
///
/// If the connection were published anyway, MPD would answer every later
/// command with `ACK [4@0] … you don't have permission` — and an ACK is
/// deliberately not connection-fatal, so nothing would ever drop that socket.
/// `ConnectionTick` short-circuits on `is_connected()`, so the app would sit
/// there reporting a healthy connection while nothing worked.
#[tokio::test]
async fn a_wrong_password_fails_the_connect_and_leaves_no_connection() {
    let (addr, _seen) = mock_mpd_requiring_password("s3cret").await;
    let client = MpdClient::new(&addr, Some("wrong".into()));

    let err = client.connect().await.expect_err("a wrong password must fail");
    assert!(
        matches!(err, crate::mpd::error::MpdError::Auth(_)),
        "expected an Auth error, got {err:?}"
    );
    assert!(
        err.to_string().to_lowercase().contains("password"),
        "the message reaches a toast, so it has to name the cause: {err}"
    );
    assert!(
        !client.is_connected().await,
        "a client that failed to authenticate must not look connected, or \
         ConnectionTick will never retry it"
    );
}

/// Every reconnect must re-authenticate. MPD authenticates a connection, not
/// a client, so the reconnect in `ConnectionTick` and the recovery that drops
/// a desynced socket both come back with no permissions unless `connect`
/// itself sends the password each time.
#[tokio::test]
async fn every_reconnect_re_sends_the_password() {
    let (addr, seen) = mock_mpd_requiring_password("s3cret").await;
    let client = MpdClient::new(&addr, Some("s3cret".into()));

    client.connect().await.expect("first connect");
    client.disconnect().await;
    client.connect().await.expect("reconnect");
    client.status().await.expect("a command after reconnecting");

    let passwords = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.starts_with("password "))
        .count();
    assert_eq!(passwords, 2, "each connection has to authenticate itself");
}

/// A server that wants no password must not be sent one — `password ""` is a
/// wrong credential to MPD, not an absent one.
#[tokio::test]
async fn no_password_configured_sends_no_password_line() {
    let (addr, seen) = mock_mpd_rejecting("0.24.0", &[]).await;
    let client = MpdClient::new(&addr, None);

    client.connect().await.expect("connect");
    client.status().await.expect("status");

    assert!(
        !seen.lock().unwrap().iter().any(|c| c.starts_with("password")),
        "nothing should have been authenticated"
    );
}

/// A dead socket must not be read as "the server doesn't support this rung".
/// Walking the ladder on a connection error would cache a weaker rung the
/// server never rejected, permanently downgrading Recently Added for the rest
/// of the session — and it would do it silently.
#[tokio::test]
async fn a_connection_error_does_not_walk_the_ladder() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    tokio::spawn(async move {
        // Greet, then hang up on the first command.
        let Ok((stream, _)) = listener.accept().await else { return };
        let (_rh, mut wh) = tokio::io::split(stream);
        use tokio::io::AsyncWriteExt;
        let _ = wh.write_all(b"OK MPD 0.24.0\n").await;
        let _ = wh.flush().await;
    });

    // Hermetic mock: no auth, and deliberately no dependence on the env var.
    let client = MpdClient::new(&addr, None);
    client.connect().await.expect("connect");
    let err = client
        .find_recently_added("2026-01-01T00:00:00Z", 10)
        .await
        .expect_err("a dropped socket must surface as an error");
    assert!(
        err.is_connection_fatal(),
        "must propagate the connection error, not degrade: {err}"
    );
}

/// The add-time walk against a real library: paging must terminate, cover
/// most albums, and produce timestamps that actually sort.
///
/// This is the expensive query in the app, so the test also prints how long
/// it took and how many pages it needed — the numbers that decide whether
/// `ADDED_PAGE` is set sensibly.
#[tokio::test]
#[ignore]
async fn live_album_added_walk_covers_the_library() {
    let Some(addr) = mpd_addr() else {
        eprintln!("skipping: set WINRMPC_TEST_MPD");
        return;
    };
    let client = MpdClient::new(&addr, mpd_password());
    client.connect().await.expect("connect to MPD");

    const PAGE: u32 = 10_000;
    let started = std::time::Instant::now();
    let mut index = std::collections::HashMap::new();
    let mut pages = 0u32;
    loop {
        let pairs = client
            .added_page(pages * PAGE, PAGE)
            .await
            .expect("added page");
        let songs = pairs.iter().filter(|(k, _)| k == "file").count() as u32;
        fold_album_added(&pairs, &mut index);
        pages += 1;
        if songs < PAGE || pages >= 40 {
            break;
        }
    }
    eprintln!(
        "{} albums in {pages} page(s), {:?}",
        index.len(),
        started.elapsed()
    );

    assert!(!index.is_empty(), "the walk found no albums at all");

    // Most album rows should have an add-time; a large shortfall means the
    // fold's key doesn't match the one the list looks up.
    let albums = client.list_albums_by_artist().await.expect("album list");
    let rows: std::collections::HashSet<String> = albums
        .iter()
        .map(|(artist, album)| {
            album_scoped_key(Some(artist), &album_base_and_disc(album).0)
        })
        .collect();
    let covered = rows.iter().filter(|k| index.contains_key(*k)).count();
    eprintln!("{covered} of {} album rows have an add-time", rows.len());
    assert!(
        covered * 10 >= rows.len() * 9,
        "under 90% coverage — the fold's key probably disagrees with the \
         album list's key"
    );

    // And the timestamps must be lexicographically comparable, which is what
    // the sort relies on.
    let mut stamps: Vec<&String> = index.values().collect();
    stamps.sort();
    assert!(
        stamps.first().unwrap() <= stamps.last().unwrap(),
        "timestamps don't order"
    );
    eprintln!("oldest: {}, newest: {}", stamps.first().unwrap(), stamps.last().unwrap());
}

/// Diagnostic, not an assertion — the fastest answer to "why does the Added
/// sort put this album in the unknown bucket".
///
/// Prints the album rows with no add-time and the index keys matching no row.
/// That output is what identified the real defect: MPD's
/// `list Album group AlbumArtist` substitutes `Artist` when the `AlbumArtist`
/// tag is absent, so a fold keyed on the raw tag produced `"\x1fHoly Diver"`
/// against a row keyed `"Dio\x1fHoly Diver"` — 348 of 801 rows matched.
#[tokio::test]
#[ignore]
async fn live_diagnose_album_added_coverage() {
    let Some(addr) = mpd_addr() else {
        eprintln!("skipping: set WINRMPC_TEST_MPD");
        return;
    };
    let client = MpdClient::new(&addr, mpd_password());
    client.connect().await.expect("connect");

    let mut index = std::collections::HashMap::new();
    let mut page = 0u32;
    loop {
        let pairs = client.added_page(page * 10_000, 10_000).await.expect("page");
        let songs = pairs.iter().filter(|(k, _)| k == "file").count() as u32;
        fold_album_added(&pairs, &mut index);
        page += 1;
        if songs < 10_000 || page >= 40 {
            break;
        }
    }

    let albums = client.list_albums_by_artist().await.expect("albums");
    let rows: std::collections::HashSet<String> = albums
        .iter()
        .map(|(a, al)| album_scoped_key(Some(a), &album_base_and_disc(al).0))
        .collect();

    let show = |k: &String| k.replace('\u{1f}', " | ");
    let missing: Vec<&String> = rows.iter().filter(|k| !index.contains_key(*k)).collect();
    let extra: Vec<&String> = index.keys().filter(|k| !rows.contains(*k)).collect();

    eprintln!(
        "{} index entries, {} album rows, {} rows without an add-time, {} keys matching no row",
        index.len(),
        rows.len(),
        missing.len(),
        extra.len()
    );
    for k in missing.iter().take(10) {
        eprintln!("  row with no add-time: {}", show(k));
    }
    for k in extra.iter().take(10) {
        eprintln!("  index key matching no row: {}", show(k));
    }
}
