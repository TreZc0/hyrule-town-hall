use crate::{
    event::{
        Data,
        InfoError,
    },
    prelude::*,
};

pub(crate) async fn info(_transaction: &mut Transaction<'_, Postgres>, _data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    // Content has been migrated to database - see event_info_content table
    // Use the WYSIWYG editor in the event setup page to manage content
    Ok(None)
}
