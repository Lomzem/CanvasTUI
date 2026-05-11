use std::{env, path::PathBuf};

use color_eyre::eyre::{Context, ContextCompat, Result, bail};
use reqwest::Url;

use crate::domain::DateRange;

#[derive(Clone, Debug)]
pub struct Config {
    pub canvas_url: Url,
    pub access_token: String,
    pub cache_path: PathBuf,
    pub initial_range: DateRange,
    pub history_chunk_days: i64,
}

impl Config {
    pub fn load(today: time::Date) -> Result<Self> {
        let canvas_url = env::var("CANVAS_URL")
            .wrap_err("missing CANVAS_URL")?
            .parse::<Url>()
            .wrap_err("CANVAS_URL must be a valid URL")?;
        let access_token =
            env::var("CANVAS_ACCESS_TOKEN").wrap_err("missing CANVAS_ACCESS_TOKEN")?;
        if access_token.trim().is_empty() {
            bail!("CANVAS_ACCESS_TOKEN must not be empty");
        }

        Ok(Self {
            canvas_url,
            access_token,
            cache_path: cache_path()?,
            initial_range: DateRange::new(today, today + time::Duration::days(60)),
            history_chunk_days: 30,
        })
    }
}

fn cache_path() -> Result<PathBuf> {
    if let Some(xdg) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(xdg).join("canvastui").join("snapshot.json"));
    }

    let home = env::var_os("HOME").wrap_err("HOME is not set and XDG_CACHE_HOME is unavailable")?;
    Ok(PathBuf::from(home)
        .join(".cache")
        .join("canvastui")
        .join("snapshot.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_xdg_cache_home_when_available() {
        unsafe {
            env::set_var("XDG_CACHE_HOME", "/tmp/cache-home");
        }

        let path = cache_path().unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/cache-home/canvastui/snapshot.json")
        );

        unsafe {
            env::remove_var("XDG_CACHE_HOME");
        }
    }
}
