use crate::prelude::*;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[cfg(windows)] #[error(transparent)] Json(#[from] serde_json::Error),
    #[error(transparent)] Wheel(#[from] wheel::Error),
    #[cfg(unix)]
    #[error("missing config file")]
    Missing,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Config {
    pub(crate) challonge: ConfigOAuth,
    pub(crate) challonge_api_key: String,
    pub(crate) discord: ConfigDiscord,
    pub(crate) league_api_key: String,
    pub(crate) ootr_api_key: String,
    pub(crate) ootr_api_key_encryption: String,
    pub(crate) racetime_bot: ConfigRaceTime,
    #[serde(rename = "racetimeOAuth")]
    pub(crate) racetime_oauth: ConfigRaceTime,
    pub(crate) secret_key: String,
    pub(crate) startgg: String,
    #[serde(rename = "startggOAuth")]
    pub(crate) startgg_oauth: ConfigOAuth,
    #[serde(default)]
    pub(crate) database: Option<ConfigDatabase>,
}

impl Config {
    pub(crate) async fn load() -> Result<Self, Error> {
        #[cfg(unix)] {
            if let Some(config_path) = BaseDirectories::new().find_config_file(if Environment::default().is_dev() { "midos-house-dev.json" } else { "midos-house.json" }) {
                Ok(fs::read_json(config_path).await?)
            } else {
                Err(Error::Missing)
            }
        }
        #[cfg(windows)] {
            Ok(fs::read_json("cfg/hth.json").await?)
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigRaceTime {
    #[serde(rename = "clientID")]
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
}

impl TypeMapKey for ConfigRaceTime {
    type Value = Self;
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigDiscord {
    #[serde(rename = "clientID")]
    pub(crate) client_id: ApplicationId,
    pub(crate) client_secret: String,
    pub(crate) bot_token: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigOAuth {
    #[serde(rename = "clientID")]
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigDatabase {
    pub(crate) host: Option<String>,
    pub(crate) port: Option<u16>,
    pub(crate) username: Option<String>,
    pub(crate) password: Option<String>,
    pub(crate) database: Option<String>,
}
