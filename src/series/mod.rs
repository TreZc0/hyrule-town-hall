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

pub(crate) mod br;
pub(crate) mod coop;
pub(crate) mod fr;
pub(crate) mod league;
pub(crate) mod mp;
pub(crate) mod mq;
pub(crate) mod mw;
pub(crate) mod mysteryd;
pub(crate) mod ndos;
pub(crate) mod ohko;
pub(crate) mod pic;
pub(crate) mod rsl;
pub(crate) mod s;
pub(crate) mod scrubs;
pub(crate) mod sgl;
pub(crate) mod soh;
pub(crate) mod tfb;
pub(crate) mod wttbb;
pub(crate) mod xkeys;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Sequence)]
pub(crate) enum Series {
    BattleRoyale,
    CoOp,
    CopaDoBrasil,
    Crosskeys,
    League,
    MixedPools,
    Mq,
    Multiworld,
    MysteryD,
    NineDaysOfSaws,
    Pictionary,
    Rsl,
    Scrubs,
    SongsOfHope,
    SpeedGaming,
    Standard,
    TournoiFrancophone,
    TriforceBlitz,
    WeTryToBeBetter,
}

impl Series {
    pub(crate) fn slug(&self) -> &'static str {
        match self {
            Self::BattleRoyale => "ohko",
            Self::CoOp => "coop",
            Self::CopaDoBrasil => "br",
            Self::Crosskeys => "xkeys",
            Self::League => "league",
            Self::MixedPools => "mp",
            Self::Mq => "mq",
            Self::Multiworld => "mw",
            Self::MysteryD => "mysteryd",
            Self::NineDaysOfSaws => "9dos",
            Self::Pictionary => "pic",
            Self::Rsl => "rsl",
            Self::Scrubs => "scrubs",
            Self::SongsOfHope => "soh",
            Self::SpeedGaming => "sgl",
            Self::Standard => "s",
            Self::TournoiFrancophone => "fr",
            Self::TriforceBlitz => "tfb",
            Self::WeTryToBeBetter => "wttbb",
        }
    }

    pub(crate) fn display_name(&self) -> &'static str {
        match self {
            Self::BattleRoyale => "Battle Royale",
            Self::CoOp => "Co-op Tournaments",
            Self::CopaDoBrasil => "Copa do Brasil",
            Self::Crosskeys => "Crosskeys Tournaments",
            Self::League => "League",
            Self::MixedPools => "Mixed Pools Tournaments",
            Self::Mq => "12 MQ Tournaments",
            Self::Multiworld => "Multiworld Tournaments",
            Self::MysteryD => "Deutsche Mystery Turniere",
            Self::NineDaysOfSaws => "9 Days of SAWS",
            Self::Pictionary => "Pictionary Spoiler Log Races",
            Self::Rsl => "Random Settings League",
            Self::Scrubs => "Scrubs Tournaments",
            Self::SongsOfHope => "Songs of Hope",
            Self::SpeedGaming => "SpeedGaming Live",
            Self::Standard => "Standard Tournaments",
            Self::TournoiFrancophone => "Tournois Francophones",
            Self::TriforceBlitz => "Triforce Blitz",
            Self::WeTryToBeBetter => "WeTryToBeBetter",
        }
    }

    pub(crate) fn default_race_duration(&self) -> TimeDelta {
        match self {
            Self::TriforceBlitz => TimeDelta::hours(2),
            Self::BattleRoyale | Self::Crosskeys | Self::MysteryD => TimeDelta::hours(2) + TimeDelta::minutes(30),
            Self::CoOp | Self::MixedPools | Self::Scrubs | Self::SpeedGaming | Self::WeTryToBeBetter => TimeDelta::hours(3),
            Self::CopaDoBrasil | Self::League | Self::NineDaysOfSaws | Self::SongsOfHope | Self::Standard | Self::TournoiFrancophone => TimeDelta::hours(3) + TimeDelta::minutes(30),
            Self::Mq | Self::Multiworld | Self::Pictionary => TimeDelta::hours(4),
            Self::Rsl => TimeDelta::hours(4) + TimeDelta::minutes(30),
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
