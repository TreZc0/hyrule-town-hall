use {
    derivative::Derivative,
    rocket::{
        form::FromFormField,
        http::uri::{
            self,
            fmt::{
                FromUriParam,
                Part,
                UriDisplay,
            },
        },
    },
    sqlx::{
        Database,
        Decode,
        Encode,
    },
    crate::prelude::*,
};

pub(crate) trait Table {
    fn query_exists(id: i64) -> sqlx::query::QueryScalar<'static, Postgres, bool, <Postgres as Database>::Arguments<'static>>;
}

pub(crate) enum Notifications {}

impl Table for Notifications {
    fn query_exists(id: i64) -> sqlx::query::QueryScalar<'static, Postgres, bool, <Postgres as Database>::Arguments<'static>> {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM notifications WHERE id = $1) AS "exists!""#, id)
    }
}

pub(crate) enum Races {}

impl Table for Races {
    fn query_exists(id: i64) -> sqlx::query::QueryScalar<'static, Postgres, bool, <Postgres as Database>::Arguments<'static>> {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM races WHERE id = $1) AS "exists!""#, id)
    }
}

pub(crate) enum Teams {}

impl Table for Teams {
    fn query_exists(id: i64) -> sqlx::query::QueryScalar<'static, Postgres, bool, <Postgres as Database>::Arguments<'static>> {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM teams WHERE id = $1) AS "exists!""#, id)
    }
}

pub(crate) enum Users {}

impl Table for Users {
    fn query_exists(id: i64) -> sqlx::query::QueryScalar<'static, Postgres, bool, <Postgres as Database>::Arguments<'static>> {
        sqlx::query_scalar!(r#"SELECT EXISTS (SELECT 1 FROM users WHERE id = $1) AS "exists!""#, id)
    }
}

#[derive(Derivative, Deserialize, Serialize)]
#[cfg_attr(unix, derive(Protocol), async_proto(where(T: Sync)))]
#[derivative(Debug(bound = ""), Clone(bound = ""), Copy(bound = ""), PartialEq(bound = ""), Eq(bound = ""), Hash(bound = ""), PartialOrd(bound = ""), Ord(bound = ""))]
#[serde(from = "u64", into = "u64")]
pub(crate) struct Id<T: Table> {
    inner: u64,
    _table: PhantomData<T>,
}

impl<T: Table> Id<T> {
    pub(crate) async fn new(transaction: &mut Transaction<'_, Postgres>) -> sqlx::Result<Self> {
        Ok(Self {
            inner: loop {
                let id = rng().random();
                if !T::query_exists(id).fetch_one(&mut **transaction).await? { break id as u64 }
            },
            _table: PhantomData,
        })
    }

    pub(crate) fn dummy() -> Self {
        Self {
            inner: 0,
            _table: PhantomData,
        }
    }
}

impl<T: Table> From<u64> for Id<T> {
    fn from(inner: u64) -> Self {
        Self {
            _table: PhantomData,
            inner,
        }
    }
}

impl<T: Table> From<i64> for Id<T> {
    fn from(id: i64) -> Self {
        Self {
            inner: id as u64,
            _table: PhantomData,
        }
    }
}

impl<T: Table> From<Id<T>> for u64 {
    fn from(Id { inner, .. }: Id<T>) -> Self {
        inner
    }
}

impl<T: Table> From<Id<T>> for i64 {
    fn from(Id { inner, .. }: Id<T>) -> Self {
        inner as Self
    }
}

impl<T: Table> FromStr for Id<T> {
    type Err = std::num::ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u64>()
            .map(Self::from)
            .or_else(|_| s.parse::<i64>().map(Self::from))
    }
}

impl<T: Table> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl<'r, T: Table, DB: Database> Decode<'r, DB> for Id<T>
where i64: Decode<'r, DB> {
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        i64::decode(value).map(|id| Self::from(id))
    }
}

impl<'q, T: Table, DB: Database> Encode<'q, DB> for Id<T>
where i64: Encode<'q, DB> {
    fn encode_by_ref(&self, buf: &mut <DB as Database>::ArgumentBuffer<'q>) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        (self.inner as i64).encode(buf)
    }

    fn encode(self, buf: &mut <DB as Database>::ArgumentBuffer<'q>) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        (self.inner as i64).encode(buf)
    }

    fn produces(&self) -> Option<<DB as Database>::TypeInfo> {
        (self.inner as i64).produces()
    }

    fn size_hint(&self) -> usize {
        Encode::size_hint(&(self.inner as i64))
    }
}

impl<T: Table, DB: Database> sqlx::Type<DB> for Id<T>
where i64: sqlx::Type<DB> {
    fn type_info() -> <DB as Database>::TypeInfo {
        i64::type_info()
    }

    fn compatible(ty: &<DB as Database>::TypeInfo) -> bool {
        i64::compatible(ty)
    }
}

impl<T: Table> sqlx::postgres::PgHasArrayType for Id<T> {
    fn array_type_info() -> sqlx::postgres::PgTypeInfo {
        i64::array_type_info()
    }
}

impl<'a, T: Table> FromParam<'a> for Id<T> {
    type Error = <u64 as FromParam<'a>>::Error;

    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        u64::from_param(param)
            .map(Self::from)
            .or_else(|_| i64::from_param(param).map(Self::from))
    }
}

impl<T: Table, P: Part> UriDisplay<P> for Id<T> {
    fn fmt(&self, f: &mut uri::fmt::Formatter<'_, P>) -> fmt::Result {
        UriDisplay::fmt(&self.inner, f)
    }
}

impl<T: Table, P: Part> FromUriParam<P, Self> for Id<T> {
    type Target = Id<T>;

    fn from_uri_param(param: Self) -> Self { param }
}

impl<'v, T: Table + Send> FromFormField<'v> for Id<T>
where i64: FromFormField<'v>, u64: FromFormField<'v> {
    fn from_value(field: form::ValueField<'v>) -> form::Result<'v, Self> {
        u64::from_value(field.clone())
            .map(Self::from)
            .or_else(|_| i64::from_value(field).map(Self::from))
    }

    fn default() -> Option<Self> { None }
}

impl From<Id<Races>> for mhstatus::RaceId {
    fn from(id: Id<Races>) -> Self {
        Self(id.inner)
    }
}
