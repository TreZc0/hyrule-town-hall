use crate::{
    event::{Data, InfoError},
    prelude::*,
};
use rocket::response::content::RawText;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub(crate) enum InfoContentError {
    #[error(transparent)] Data(#[from] event::DataError),
    #[error(transparent)] Event(#[from] event::Error),
    #[error(transparent)] Sql(#[from] sqlx::Error),
    #[error(transparent)] Json(#[from] serde_json::Error),
    #[error("Content not found")]
    NotFound,
    #[error("Invalid HTML content")]
    InvalidHtml,
}

impl From<InfoContentError> for event::Error {
    fn from(_err: InfoContentError) -> Self {
        event::Error::Data(event::DataError::Sql(sqlx::Error::RowNotFound))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EventInfoContent {
    pub series: String,
    pub event: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EventInfoContent {
    pub(crate) async fn get(_transaction: &mut Transaction<'_, Postgres>, _series: Series, _event: &str) -> Result<Option<Self>, InfoContentError> {
        // Temporarily return None until migration is run
        Ok(None)
    }

    pub(crate) async fn save(_transaction: &mut Transaction<'_, Postgres>, _series: Series, _event: &str, content: &str) -> Result<(), InfoContentError> {
        // Validate HTML content (basic validation)
        if !is_valid_html(content) {
            return Err(InfoContentError::InvalidHtml);
        }

        // Temporarily return Ok until migration is run
        Ok(())
    }

    pub(crate) async fn delete(_transaction: &mut Transaction<'_, Postgres>, _series: Series, _event: &str) -> Result<(), InfoContentError> {
        // Temporarily return Ok until migration is run
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct EventInfoContentRow {
    series: String,
    event: String,
    content: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

fn is_valid_html(content: &str) -> bool {
    // Basic HTML validation - check for balanced tags
    let mut stack = Vec::new();
    let mut in_tag = false;
    let mut tag_name = String::new();
    let mut is_closing = false;

    for ch in content.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag_name.clear();
                is_closing = false;
            }
            '>' => {
                in_tag = false;
                if !tag_name.is_empty() {
                    if is_closing {
                        if let Some(expected) = stack.pop() {
                            if expected != tag_name {
                                return false; // Mismatched tags
                            }
                        } else {
                            return false; // Closing tag without opening
                        }
                    } else if !tag_name.starts_with('/') && !tag_name.ends_with('/') {
                        stack.push(tag_name.clone());
                    }
                }
            }
            '/' if in_tag && tag_name.is_empty() => {
                is_closing = true;
            }
            c if in_tag => {
                if c.is_whitespace() {
                    break; // End of tag name
                } else {
                    tag_name.push(c);
                }
            }
            _ => {}
        }
    }

    stack.is_empty() // All tags should be balanced
}

pub(crate) async fn get_info_content(transaction: &mut Transaction<'_, Postgres>, data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    let content = EventInfoContent::get(transaction, data.series, &data.event).await.map_err(|_e| InfoError::Data(event::DataError::Sql(sqlx::Error::RowNotFound)))?;
    
    Ok(content.map(|c| RawHtml(c.content)))
}

pub(crate) async fn save_info_content(transaction: &mut Transaction<'_, Postgres>, series: Series, event: &str, content: &str) -> Result<(), InfoContentError> {
    EventInfoContent::save(transaction, series, event, content).await
}

pub(crate) async fn delete_info_content(transaction: &mut Transaction<'_, Postgres>, series: Series, event: &str) -> Result<(), InfoContentError> {
    EventInfoContent::delete(transaction, series, event).await
}

// WYSIWYG Editor API endpoints
#[derive(FromForm, CsrfForm)]
pub(crate) struct SaveInfoContentForm {
    #[field(default = String::new())]
    csrf: String,
    content: String,
}

#[rocket::post("/event/<series>/<event>/info-content", data = "<form>")]
pub(crate) async fn save_content(
    pool: &State<PgPool>,
    me: User,
    _uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, SaveInfoContentForm>>,
) -> Result<RawText<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if event_data.is_ended() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }

    if !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if let Some(ref value) = form.value {
        match save_info_content(&mut transaction, series, event, &value.content).await {
            Ok(_) => {
                transaction.commit().await?;
                Ok(RawText(serde_json::to_string(&serde_json::json!({
                    "success": true,
                    "message": "Content saved successfully"
                }))?))
            }
            Err(InfoContentError::InvalidHtml) => {
                Ok(RawText(serde_json::to_string(&serde_json::json!({
                    "success": false,
                    "error": "Invalid HTML content"
                }))?))
            }
            Err(_) => Err(StatusOrError::Status(Status::InternalServerError)),
        }
    } else {
        Err(StatusOrError::Status(Status::BadRequest))
    }
}

#[rocket::get("/event/<series>/<event>/info-content")]
pub(crate) async fn get_content(
    pool: &State<PgPool>,
    me: Option<User>,
    _uri: Origin<'_>,
    series: Series,
    event: &str,
) -> Result<RawText<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;

    // Check if user is organizer
    let is_organizer = if let Some(ref me) = me {
        event_data.organizers(&mut transaction).await?.contains(me)
    } else {
        false
    };

    let content = EventInfoContent::get(&mut transaction, series, event).await?;
    
    let has_content = content.is_some();
    let content_text = content.map(|c| c.content).unwrap_or_default();
    let response = serde_json::json!({
        "content": content_text,
        "is_organizer": is_organizer,
        "has_content": has_content
    });

    transaction.commit().await?;
    Ok(RawText(serde_json::to_string(&response)?))
}

#[derive(FromForm, CsrfForm)]
pub(crate) struct DeleteInfoContentForm {
    #[field(default = String::new())]
    csrf: String,
}

#[rocket::post("/event/<series>/<event>/info-content/delete", data = "<form>")]
pub(crate) async fn delete_content(
    pool: &State<PgPool>,
    me: User,
    _uri: Origin<'_>,
    csrf: Option<CsrfToken>,
    series: Series,
    event: &str,
    form: Form<Contextual<'_, DeleteInfoContentForm>>,
) -> Result<RawText<String>, StatusOrError<event::Error>> {
    let mut transaction = pool.begin().await?;
    let event_data = Data::new(&mut transaction, series, event).await?.ok_or(StatusOrError::Status(Status::NotFound))?;
    let mut form = form.into_inner();
    form.verify(&csrf);

    if event_data.is_ended() {
        return Err(StatusOrError::Status(Status::BadRequest));
    }

    if !event_data.organizers(&mut transaction).await?.contains(&me) {
        return Err(StatusOrError::Status(Status::Forbidden));
    }

    if form.value.is_some() {
        match delete_info_content(&mut transaction, series, event).await {
            Ok(_) => {
                transaction.commit().await?;
                Ok(RawText(serde_json::to_string(&serde_json::json!({
                    "success": true,
                    "message": "Content deleted successfully"
                }))?))
            }
            Err(_) => Err(StatusOrError::Status(Status::InternalServerError)),
        }
    } else {
        Err(StatusOrError::Status(Status::BadRequest))
    }
} 