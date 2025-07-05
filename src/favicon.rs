use {
    std::fmt::Write as _,
    derive_more::From,
    image::{
        GenericImage as _,
        ImageReader,
        RgbaImage,
    },
    ootr_utils::camc::ChestAppearance,
    rocket::{
        fs::NamedFile,
        http::{
            impl_from_uri_param_identity,
            uri::{
                self,
                fmt::{
                    Path,
                    UriDisplay,
                },
            },
        },
        request::FromParam,
    },
    rocket_util::Response,
    crate::{
        hash_icon::SpoilerLog,
        prelude::*,
    },
};

#[derive(Clone, Copy, Deserialize)]
#[serde(transparent)]
pub(crate) struct ChestAppearances(pub(crate) [ChestAppearance; 4]);

impl ChestAppearances {
    pub(crate) const VANILLA: Self = Self([ChestAppearance { texture: ChestTexture::Normal, big: false }; 4]);
    pub(crate) const INVISIBLE: Self = Self([ChestAppearance { texture: ChestTexture::Invisible, big: false }; 4]);
    pub(crate) const SMALL_KEYS: Self = Self([ChestAppearance { texture: ChestTexture::SmallKey1751, big: false }; 4]);
    pub(crate) const TOKENS: Self = Self([ChestAppearance { texture: ChestTexture::Token, big: false }; 4]);

    pub(crate) fn random() -> Self {
        //TODO automatically keep up to date with the dev-mvp branch of the RSL script:
        // ootrstats-supervisor --rsl --github-user=fenhl --branch=dev-mvp midos-house assets/chests-rsl-dev-mvp.json
        static WEIGHTS: LazyLock<Vec<(ChestAppearances, usize)>> = LazyLock::new(|| serde_json::from_str(include_str!("../assets/chests-rsl-dev-mvp.json")).expect("failed to parse chest weights"));

        WEIGHTS.choose_weighted(&mut rng(), |(_, weight)| *weight).expect("failed to choose random chest textures").0
    }

    pub(crate) fn textures(self) -> ChestTextures {
        ChestTextures(self.0.map(|ChestAppearance { texture, .. }| texture))
    }
}

impl From<SpoilerLog> for ChestAppearances {
    fn from(_spoiler: SpoilerLog) -> Self {
        ChestAppearances::VANILLA
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ChestTextures(pub(crate) [ChestTexture; 4]);

#[derive(Debug, thiserror::Error, From)]
pub(crate) enum ChestTexturesFromParamError {
    #[error("expected 4 characters, got {}", .0.len())]
    Len(Vec<ChestTexture>),
    #[error("unknown chest texture abbreviation: {0}")]
    Texture(char),
}

impl<'a> FromParam<'a> for ChestTextures {
    type Error = ChestTexturesFromParamError;

    fn from_param(param: &'a str) -> Result<Self, ChestTexturesFromParamError> {
        Ok(Self(param.chars().map(ChestTexture::try_from).try_collect::<_, Vec<_>, _>()?.try_into()?))
    }
}

impl UriDisplay<Path> for ChestTextures {
    fn fmt(&self, f: &mut uri::fmt::Formatter<'_, Path>) -> fmt::Result {
        write!(f, "{}", self.0.iter().copied().map_into::<char>().format(""))
    }
}

impl_from_uri_param_identity!([Path] ChestTextures);

#[rocket::get("/favicon.ico")]
pub(crate) async fn favicon_ico() -> io::Result<NamedFile> {
    NamedFile::open("assets/favicon.ico").await
}

#[derive(Debug, thiserror::Error, rocket_util::Error)]
pub(crate) enum FaviconError {
    #[error(transparent)] Image(#[from] image::ImageError),
    #[error(transparent)] Io(#[from] io::Error),
    #[error("unsupported file extension")]
    UnsupportedSuffix,
}

#[rocket::get("/favicon/<textures_ext>")]
pub(crate) async fn favicon_png(textures_ext: Suffix<'_, ChestTextures>) -> Result<Response<RgbaImage>, FaviconError> {
    let Suffix(ChestTextures([top_left, top_right, bottom_left, bottom_right]), "png") = textures_ext else { return Err(FaviconError::UnsupportedSuffix) };
    let mut buf = RgbaImage::new(1024, 1024);
    buf.copy_from(&ImageReader::open(format!("assets/static/chest/{}.png", char::from(top_left)))?.decode()?, 0, 0)?;
    buf.copy_from(&ImageReader::open(format!("assets/static/chest/{}.png", char::from(top_right)))?.decode()?, 512, 0)?;
    buf.copy_from(&ImageReader::open(format!("assets/static/chest/{}.png", char::from(bottom_left)))?.decode()?, 0, 512)?;
    buf.copy_from(&ImageReader::open(format!("assets/static/chest/{}.png", char::from(bottom_right)))?.decode()?, 512, 512)?;
    Ok(Response(buf))
}
