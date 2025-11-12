#![allow(dead_code)]
//! Serialization and deserialization of data using bincode.
//! Used mainly for caching data during testing.

use serde::{Deserialize, Serialize};
use std::io::{BufReader, BufWriter};

use crate::{error, log};

#[derive(Serialize, Deserialize)]
pub struct SerdeWrapper<T>(pub T);

/// Save a serializable value to a file using bincode.
/// Used for testing without constantly refetching data.
/// Exits the program on failure.
///
/// Example:
/// `serialize::save_cache("cache/feed.bin", &channel);`
pub fn save_cache<T, P>(path: P, value: &T)
where
    T: serde::Serialize,
    P: AsRef<std::path::Path>,
{
    let save = || -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| {
            error!("Failed to create cache file: {}", e);
            e.to_string()
        })?;
        let mut writer = BufWriter::new(file);
        bincode::serde::encode_into_std_write(value, &mut writer, bincode::config::standard())
            .map_err(|e| {
                error!("Failed to encode cache data: {}", e);
                e.to_string()
            })?;
        Ok(())
    };

    if let Err(e) = save() {
        error!("Failed to save cache: {}", e);
        std::process::exit(1);
    }
}

/// Load a deserializable value from a file using bincode.
/// Used for testing without constantly refetching data.
///
/// Example:
/// `let channel: rss::Channel = serialize::load_cache("cache/feed.bin");`
pub fn load_cache<T, P>(path: P) -> T
where
    T: for<'de> serde::de::DeserializeOwned,
    P: AsRef<std::path::Path>,
{
    let load = || -> Result<SerdeWrapper<T>, String> {
        let file = std::fs::File::open(path).map_err(|e| {
            error!("Failed to open cache file: {}", e);
            e.to_string()
        })?;
        let mut reader = BufReader::new(file);
        let decoded: SerdeWrapper<T> =
            bincode::serde::decode_from_std_read(&mut reader, bincode::config::standard())
                .map_err(|e| {
                    error!("Failed to decode cache data: {}", e);
                    e.to_string()
                })?;
        Ok(decoded)
    };

    match load() {
        Ok(t) => t.0,
        Err(e) => {
            error!("Failed to load cache: {}", e);
            std::process::exit(1);
        }
    }
}
