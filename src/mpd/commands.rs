//! Parse raw MPD key-value responses into typed structs.

use crate::mpd::error::MpdResult;
use crate::mpd::protocol::{pairs_to_map, split_groups};
use crate::mpd::types::*;
use std::collections::HashMap;
use std::time::Duration;

pub fn parse_status(pairs: &[(String, String)]) -> MpdResult<Status> {
    let map = pairs_to_map(pairs);
    let get = |k: &str| map.get(k).and_then(|v| v.first()).map(|s| s.as_str());
    let get_bool = |k: &str| get(k).map(|v| v == "1").unwrap_or(false);
    let get_u32 = |k: &str| -> Option<u32> { get(k).and_then(|v| v.parse().ok()) };
    let get_f64 = |k: &str| -> Option<f64> { get(k).and_then(|v| v.parse().ok()) };

    Ok(Status {
        volume: get("volume").and_then(|v| v.parse().ok()).unwrap_or(-1),
        repeat: get_bool("repeat"),
        random: get_bool("random"),
        single: match get("single") {
            Some("1") => SingleState::On,
            Some("oneshot") => SingleState::Oneshot,
            _ => SingleState::Off,
        },
        consume: match get("consume") {
            Some("1") => ConsumeState::On,
            Some("oneshot") => ConsumeState::Oneshot,
            _ => ConsumeState::Off,
        },
        queue_version: get_u32("playlist").unwrap_or(0),
        queue_length: get_u32("playlistlength").unwrap_or(0),
        state: match get("state") {
            Some("play") => PlayState::Play,
            Some("pause") => PlayState::Pause,
            _ => PlayState::Stop,
        },
        song_pos: get_u32("song"),
        song_id: get_u32("songid"),
        next_song_pos: get_u32("nextsong"),
        next_song_id: get_u32("nextsongid"),
        elapsed: get_f64("elapsed").map(Duration::from_secs_f64),
        duration: get_f64("duration").map(Duration::from_secs_f64),
        bitrate: get_u32("bitrate"),
        crossfade: get_u32("xfade"),
        mixrampdb: get_f64("mixrampdb"),
        audio: get("audio").and_then(|a| {
            let p: Vec<&str> = a.split(':').collect();
            if p.len() == 3 {
                Some(AudioFormat {
                    sample_rate: p[0].parse().ok()?,
                    bits: p[1].parse().ok()?,
                    channels: p[2].parse().ok()?,
                })
            } else {
                None
            }
        }),
        updating_db: get_u32("updating_db"),
        error: get("error").map(|s| s.to_string()),
        partition: get("partition").map(|s| s.to_string()),
    })
}

pub fn parse_song(pairs: &[(String, String)]) -> Song {
    let mut song = Song::default();
    for (k, v) in pairs {
        match k.as_str() {
            "file" => song.file = v.clone(),
            "Title" => song.title = Some(v.clone()),
            "Artist" => song.artist = Some(v.clone()),
            "Album" => song.album = Some(v.clone()),
            "AlbumArtist" => song.album_artist = Some(v.clone()),
            "Genre" => song.genre = Some(v.clone()),
            "Date" => song.date = Some(v.clone()),
            "Track" => song.track = Some(v.clone()),
            "Disc" => song.disc = Some(v.clone()),
            "duration" => song.duration_secs = v.parse().ok(),
            "Time" => {
                if song.duration_secs.is_none() {
                    song.duration_secs = v.parse::<u64>().ok().map(|s| s as f64);
                }
            }
            "Pos" => song.pos = v.parse().ok(),
            "Id" => song.id = v.parse().ok(),
            "Last-Modified" => song.last_modified = Some(v.clone()),
            "Composer" => song.composer = Some(v.clone()),
            "Performer" => song.performer = Some(v.clone()),
            "Comment" => song.comment = Some(v.clone()),
            "Name" => song.name = Some(v.clone()),
            _ => {
                song.tags.entry(k.clone()).or_default().push(v.clone());
            }
        }
    }
    song
}

pub fn parse_songs(pairs: &[(String, String)]) -> Vec<Song> {
    split_groups(pairs, "file")
        .into_iter()
        .map(|g| parse_song(&g))
        .collect()
}

pub fn parse_outputs(pairs: &[(String, String)]) -> Vec<Output> {
    split_groups(pairs, "outputid")
        .into_iter()
        .map(|group| {
            let map = pairs_to_map(&group);
            let get =
                |k: &str| map.get(k).and_then(|v| v.first()).cloned().unwrap_or_default();
            Output {
                id: get("outputid").parse().unwrap_or(0),
                name: get("outputname"),
                plugin: get("plugin"),
                enabled: get("outputenabled") == "1",
                attributes: HashMap::new(),
            }
        })
        .collect()
}

pub fn parse_partitions(pairs: &[(String, String)]) -> Vec<Partition> {
    split_groups(pairs, "partition")
        .into_iter()
        .map(|group| {
            let name = group
                .iter()
                .find(|(k, _)| k == "partition")
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            Partition { name }
        })
        .collect()
}

pub fn parse_directory_listing(pairs: &[(String, String)]) -> Vec<DirectoryEntry> {
    let mut entries = Vec::new();
    let mut cur_type: Option<&str> = None;
    let mut cur_pairs: Vec<(String, String)> = Vec::new();

    for (k, v) in pairs {
        match k.as_str() {
            "file" | "directory" | "playlist" => {
                if let Some(ct) = cur_type {
                    entries.push(finish_dir_entry(ct, &cur_pairs));
                }
                cur_type = Some(k.as_str());
                cur_pairs = vec![(k.clone(), v.clone())];
            }
            _ => cur_pairs.push((k.clone(), v.clone())),
        }
    }
    if let Some(ct) = cur_type {
        entries.push(finish_dir_entry(ct, &cur_pairs));
    }
    entries
}

fn finish_dir_entry(entry_type: &str, pairs: &[(String, String)]) -> DirectoryEntry {
    let find = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };

    match entry_type {
        "file" => DirectoryEntry::File(parse_song(pairs)),
        "directory" => DirectoryEntry::Directory(DirectoryInfo {
            path: find("directory").unwrap_or_default(),
            last_modified: find("Last-Modified"),
        }),
        _ => DirectoryEntry::Playlist(PlaylistInfo {
            name: find("playlist").unwrap_or_default(),
            last_modified: find("Last-Modified"),
        }),
    }
}

pub fn parse_stats(pairs: &[(String, String)]) -> MpdResult<Stats> {
    let map = pairs_to_map(pairs);
    let get_u64 = |k: &str| -> u64 {
        map.get(k)
            .and_then(|v| v.first())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };
    Ok(Stats {
        uptime: Duration::from_secs(get_u64("uptime")),
        playtime: Duration::from_secs(get_u64("playtime")),
        artists: get_u64("artists"),
        albums: get_u64("albums"),
        songs: get_u64("songs"),
        db_playtime: Duration::from_secs(get_u64("db_playtime")),
        db_update: get_u64("db_update"),
    })
}

pub fn parse_tag_list(pairs: &[(String, String)], tag: &str) -> Vec<String> {
    pairs
        .iter()
        .filter(|(k, _)| k == tag)
        .map(|(_, v)| v.clone())
        .collect()
}
