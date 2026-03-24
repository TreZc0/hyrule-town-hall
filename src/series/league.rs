use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(match &*data.event {
        "8" => Some(html! {
            article {
                p {
                    : "This is OoTR League season 8, organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://docs.google.com/document/d/101R4sZqZpslI0E77sU4jtBB5fEy0RSpEnZ99PnoUV1k/edit") : "the rules document"; //TODO import text
                    : " for details.";
                }
            }
        }),
        _ => Some(html! {
            article {
                p {
                    : "This is OoTR League season ";
                    : data.event;
                    : ", organized by ";
                    : English.join_html_opt(data.organizers(transaction).await?);
                    : ". See ";
                    a(href = "https://league.ootrandomizer.com/") : "the official website";
                    : " for details.";
                }
            }
        }),
    })
}

#[derive(Deserialize)]
#[serde(transparent)]
struct JsonScheduleVersion(u8);

#[derive(Debug, thiserror::Error)]
#[error("expected League schedule format version 1, got version {0}")]
struct ScheduleVersionMismatch(u8);

impl TryFrom<JsonScheduleVersion> for ScheduleVersion {
    type Error = ScheduleVersionMismatch;

    fn try_from(JsonScheduleVersion(version): JsonScheduleVersion) -> Result<Self, ScheduleVersionMismatch> {
        if version == 1 {
            Ok(Self)
        } else {
            Err(ScheduleVersionMismatch(version))
        }
    }
}

#[derive(Deserialize)]
#[serde(try_from = "JsonScheduleVersion")]
struct ScheduleVersion;

#[derive(Deserialize)]
pub(crate) struct Schedule {
    #[allow(unused)] // version check
    version: ScheduleVersion,
    pub(crate) matches: Vec<Match>,
}

fn deserialize_datetime<'de, D: Deserializer<'de>>(deserializer: D) -> Result<DateTime<Utc>, D::Error> {
    // workaround for https://github.com/chronotope/chrono/issues/330
    Ok(NaiveDateTime::parse_from_str(&format!("{}:00", <&str>::deserialize(deserializer)?), "%Y-%m-%d %H:%M:%S").map_err(D::Error::custom)?.and_utc())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Match {
    #[serde(rename = "timeUTC", deserialize_with = "deserialize_datetime")]
    pub(crate) time_utc: DateTime<Utc>,
    pub(crate) player_a: User,
    pub(crate) player_b: User,
    pub(crate) id: i32,
    pub(crate) division: String,
    pub(crate) status: MatchStatus,
    pub(crate) restreamers: Vec<User>,
    pub(crate) restream_language: Option<Language>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct User {
    pub(crate) discord_id: Option<UserId>,
    /// Not deserialized as a URL since the League API may return strings that aren't valid URLs
    pub(crate) racetime_url: Option<String>,
    pub(crate) username: String,
    pub(crate) twitch_username: Option<String>,
}

impl User {
    pub(crate) async fn racetime_id(&self, http_client: &reqwest::Client) -> wheel::Result<Option<String>> {
        let url_part = self.racetime_url.as_deref().and_then(|url| {
            let (_, id) = regex_captures!("^https://racetime.gg/user/([0-9A-Za-z]+)(?:/.*)?$", url)?;
            Some(id.to_owned())
        });
        Ok(if let Some(url_part) = url_part {
            racetime_bot::user_data(http_client, &url_part).await?.map(|user_data| user_data.id)
        } else {
            None
        })
    }

    pub(crate) async fn into_entrant(self, http_client: &reqwest::Client) -> wheel::Result<Entrant> {
        Ok(if let Some(id) = self.discord_id {
            Entrant::Discord {
                racetime_id: self.racetime_id(http_client).await?,
                twitch_username: self.twitch_username,
                id,
            }
        } else {
            Entrant::Named {
                racetime_id: self.racetime_id(http_client).await?,
                name: self.username,
                twitch_username: self.twitch_username,
            }
        })
    }
}

#[derive(Deserialize)]
pub(crate) enum MatchStatus {
    Canceled,
    Confirmed,
}
