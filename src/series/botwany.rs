use crate::{
    event::{Data, InfoError},
    prelude::*,
};

pub(crate) async fn info(_transaction: &mut Transaction<'_, Postgres>, _data: &Data<'_>) -> Result<Option<RawHtml<String>>, InfoError> {
    Ok(None)
}
