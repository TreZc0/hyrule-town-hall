use {
    anyhow::anyhow,
    rocket::http::{
        impl_from_uri_param_identity,
        uri::{
            self,
            fmt::{
                Path,
                UriDisplay,
            },
        },
    },
    sqlx::{
        Decode,
        Encode,
        postgres::{
            PgArgumentBuffer,
            PgTypeInfo,
            PgValueRef,
        },
    },
    crate::prelude::*,
    chrono::TimeDelta,
};

pub(crate) mod s;
pub(crate) mod xkeys;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Sequence)]
pub(crate) enum Series {
    Crosskeys,
    Standard,
}

impl Series {
    pub(crate) fn slug(&self) -> &'static str {
        match self {
            Self::Crosskeys => "xkeys",
            Self::Standard => "s",
        }
    }

    pub(crate) fn display_name(&self) -> &'static str {
        match self {
            Self::Crosskeys => "Crosskeys Tournaments",
            Self::Standard => "Standard Tournaments",
        }
    }

    pub(crate) fn default_race_duration(&self) -> TimeDelta {
        match self {
            Self::Crosskeys => TimeDelta::hours(2) + TimeDelta::minutes(30),
            Self::Standard => TimeDelta::hours(3) + TimeDelta::minutes(30),
        }
    }
}

impl FromStr for Series {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        all::<Self>().find(|series| series.slug() == s).ok_or(())
    }
}

impl<'r> Decode<'r, Postgres> for Series {
    fn decode(value: PgValueRef<'r>) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let series = <&str as Decode<Postgres>>::decode(value)?;
        series.parse().map_err(|()| anyhow!("unknown series: {series}").into())
    }
}

impl<'q> Encode<'q, Postgres> for Series {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        Encode::<Postgres>::encode_by_ref(&self.slug(), buf)
    }

    fn encode(self, buf: &mut PgArgumentBuffer) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        Encode::<Postgres>::encode(self.slug(), buf)
    }

    fn produces(&self) -> Option<PgTypeInfo> {
        Encode::<Postgres>::produces(&self.slug())
    }

    fn size_hint(&self) -> usize {
        Encode::<Postgres>::size_hint(&self.slug())
    }
}

impl sqlx::Type<Postgres> for Series {
    fn type_info() -> PgTypeInfo {
        <&str as sqlx::Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        <&str as sqlx::Type<Postgres>>::compatible(ty)
    }
}

impl<'a> FromParam<'a> for Series {
    type Error = &'a str;

    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        param.parse().map_err(|()| param)
    }
}

impl UriDisplay<Path> for Series {
    fn fmt(&self, f: &mut uri::fmt::Formatter<'_, Path>) -> fmt::Result {
        UriDisplay::fmt(self.slug(), f) // assume all series names are URI safe
    }
}

impl_from_uri_param_identity!([Path] Series);

impl fmt::Display for Series {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slug())
    }
}
