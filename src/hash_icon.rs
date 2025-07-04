use {
    std::num::NonZeroU8,
    async_proto::Protocol,
    enum_iterator::Sequence,
    serde::{
        Deserialize,
        Serialize,
    },
    serde_plain::{
        derive_display_from_serialize,
        derive_fromstr_from_deserialize,
    },
};

#[derive(Debug, Deserialize, Protocol)]
pub struct SpoilerLog {
    pub file_hash: [HashIcon; 5],
    pub password: Option<[OcarinaNote; 6]>,
    pub settings: Vec<Settings>,
}

fn make_one() -> NonZeroU8 { NonZeroU8::new(1).unwrap() }

#[derive(Debug, Deserialize, Protocol)]
pub struct Settings {
    #[serde(default = "make_one")]
    pub world_count: NonZeroU8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Sequence, Deserialize, Serialize, Protocol)]
pub enum OcarinaNote {
    A,
    #[serde(rename = "C down")]
    CDown,
    #[serde(rename = "C right")]
    CRight,
    #[serde(rename = "C left")]
    CLeft,
    #[serde(rename = "C up")]
    CUp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Sequence, Deserialize, Serialize, Protocol, sqlx::Type)]
#[sqlx(type_name = "hash_icon")]
pub(crate) enum HashIcon {
   Bomb,
   Bombos,
   Boomerang,
   Bow,
   Hookshot,
   Mushroom,
   Pendant,
   Powder,
   Rod,
   Ether,
   Quake,
   Lamp,
   Hammer,
   Shovel,
   Ocarina,
   #[serde(rename = "Bug Net")]
   #[sqlx(rename = "Bug Net")]
   BugNet,
   Book,
   Bottle,
   Potion,
   Cane,
   Cape,
   Mirror,
   Boots,
   Gloves,
   Flippers,
   Pearl,
   Shield,
   Tunic,
   Heart,
   Map,
   Compass,
   Key,
}

impl HashIcon {
    pub fn from_racetime_emoji(emoji: &str) -> Option<Self> {
        match emoji {
            "HashBombs" => Some(Self::Bomb),
            "HashBombos" => Some(Self::Bombos),
            "HashBoomerang" => Some(Self::Boomerang),
            "HashBow" => Some(Self::Bow),
            "HashHookshot" => Some(Self::Hookshot),
            "HashMushroom" => Some(Self::Mushroom),
            "HashPendant" => Some(Self::Pendant),
            "HashMagicPowder" => Some(Self::Powder),
            "HashIceRod" => Some(Self::Rod),
            "HashEther" => Some(Self::Ether),
            "HashQuake" => Some(Self::Quake),
            "HashLamp" => Some(Self::Lamp),
            "HashHammer" => Some(Self::Hammer),
            "HashShovel" => Some(Self::Shovel),
            "HashFlute" => Some(Self::Ocarina),
            "HashBugnet" => Some(Self::BugNet),
            "HashBook" => Some(Self::Book),
            "HashGreenPotion" => Some(Self::Bottle),
            "HashPotion" => Some(Self::Potion),
            "HashSomaria" => Some(Self::Cane),
            "HashCape" => Some(Self::Cape),
            "HashMirror" => Some(Self::Mirror),
            "HashBoots" => Some(Self::Boots),
            "HashGloves" => Some(Self::Gloves),
            "HashFlippers" => Some(Self::Flippers),
            "HashMoonPearl" => Some(Self::Pearl),
            "HashShield" => Some(Self::Shield),
            "HashTunic" => Some(Self::Tunic),
            "HashHeart" => Some(Self::Heart),
            "HashMap" => Some(Self::Map),
            "HashCompass" => Some(Self::Compass),
            "HashKey" => Some(Self::Key),
            _ => None,
        }
    }

    pub fn to_racetime_emoji(&self) -> &'static str {
        match self {
            Self::Bomb => "HashBombs",
            Self::Bombos => "HashBombos",
            Self::Boomerang => "HashBoomerang",
            Self::Bow => "HashBow",
            Self::Hookshot => "HashHookshot",
            Self::Mushroom => "HashMushroom",
            Self::Pendant => "HashPendant",
            Self::Powder => "HashMagicPowder",
            Self::Rod => "HashIceRod",
            Self::Ether => "HashEther",
            Self::Quake => "HashQuake",
            Self::Lamp => "HashLamp",
            Self::Hammer => "HashHammer",
            Self::Shovel => "HashShovel",
            Self::Ocarina => "HashFlute",
            Self::BugNet => "HashBugnet",
            Self::Book => "HashBook",
            Self::Bottle => "HashGreenPotion",
            Self::Potion => "HashPotion",
            Self::Cane => "HashSomaria",
            Self::Cape => "HashCape",
            Self::Mirror => "HashMirror",
            Self::Boots => "HashBoots",
            Self::Gloves => "HashGloves",
            Self::Flippers => "HashFlippers",
            Self::Pearl => "HashMoonPearl",
            Self::Shield => "HashShield",
            Self::Tunic => "HashTunic",
            Self::Heart => "HashHeart",
            Self::Map => "HashMap",
            Self::Compass => "HashCompass",
            Self::Key => "HashKey",
        }
    }
}

derive_fromstr_from_deserialize!(HashIcon);
derive_display_from_serialize!(HashIcon);

#[derive(Debug, thiserror::Error)]
#[error("invalid hash icon")]
pub struct HashIconDecodeError;

impl TryFrom<u8> for HashIcon {
    type Error = HashIconDecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::Bomb),
            0x01 => Ok(Self::Bombos),
            0x02 => Ok(Self::Boomerang),
            0x03 => Ok(Self::Bow),
            0x04 => Ok(Self::Hookshot),
            0x05 => Ok(Self::Mushroom),
            0x06 => Ok(Self::Pendant),
            0x07 => Ok(Self::Powder),
            0x08 => Ok(Self::Rod),
            0x09 => Ok(Self::Ether),
            0x0A => Ok(Self::Quake),
            0x0B => Ok(Self::Lamp),
            0x0C => Ok(Self::Hammer),
            0x0D => Ok(Self::Shovel),
            0x0E => Ok(Self::Ocarina),
            0x0F => Ok(Self::BugNet),
            0x10 => Ok(Self::Book),
            0x11 => Ok(Self::Bottle),
            0x12 => Ok(Self::Potion),
            0x13 => Ok(Self::Cane),
            0x14 => Ok(Self::Cape),
            0x15 => Ok(Self::Mirror),
            0x16 => Ok(Self::Boots),
            0x17 => Ok(Self::Gloves),
            0x18 => Ok(Self::Flippers),
            0x19 => Ok(Self::Pearl),
            0x1A => Ok(Self::Shield),
            0x1B => Ok(Self::Tunic),
            0x1C => Ok(Self::Heart),
            0x1D => Ok(Self::Map),
            0x1E => Ok(Self::Compass),
            0x1F => Ok(Self::Key),
            _ => Err(HashIconDecodeError),
        }
    }
}

impl From<HashIcon> for u8 {
    fn from(icon: HashIcon) -> Self {
        match icon {
            HashIcon::Bomb => 0x00,
            HashIcon::Bombos => 0x01,
            HashIcon::Boomerang => 0x02,
            HashIcon::Bow => 0x03,
            HashIcon::Hookshot => 0x04,
            HashIcon::Mushroom => 0x05,
            HashIcon::Pendant => 0x06,
            HashIcon::Powder => 0x07,
            HashIcon::Rod => 0x08,
            HashIcon::Ether => 0x09,
            HashIcon::Quake => 0x0A,
            HashIcon::Lamp => 0x0B,
            HashIcon::Hammer => 0x0C,
            HashIcon::Shovel => 0x0D,
            HashIcon::Ocarina => 0x0E,
            HashIcon::BugNet => 0x0F,
            HashIcon::Book => 0x10,
            HashIcon::Bottle => 0x11,
            HashIcon::Potion => 0x12,
            HashIcon::Cane => 0x13,
            HashIcon::Cape => 0x14,
            HashIcon::Mirror => 0x15,
            HashIcon::Boots => 0x16,
            HashIcon::Gloves => 0x17,
            HashIcon::Flippers => 0x18,
            HashIcon::Pearl => 0x19,
            HashIcon::Shield => 0x1A,
            HashIcon::Tunic => 0x1B,
            HashIcon::Heart => 0x1C,
            HashIcon::Map => 0x1D,
            HashIcon::Compass => 0x1E,
            HashIcon::Key => 0x1F,
        }
    }
}

impl OcarinaNote {
    pub fn from_racetime_emoji(emoji: &str) -> Option<Self> {
        match emoji {
            "NoteA" => Some(Self::A),
            "NoteCdown" => Some(Self::CDown),
            "NoteCright" => Some(Self::CRight),
            "NoteCleft" => Some(Self::CLeft),
            "NoteCup" => Some(Self::CUp),
            _ => None,
        }
    }

    pub fn to_racetime_emoji(&self) -> &'static str {
        match self {
            Self::A => "NoteA",
            Self::CDown => "NoteCdown",
            Self::CRight => "NoteCright",
            Self::CLeft => "NoteCleft",
            Self::CUp => "NoteCup",
        }
    }
}

impl TryFrom<char> for OcarinaNote {
    type Error = char;

    fn try_from(c: char) -> Result<Self, char> {
        match c {
            'A' | 'a' => Ok(Self::A),
            'D' | 'd' | 'V' | 'v' => Ok(Self::CDown),
            'R' | 'r' | '>' => Ok(Self::CRight),
            'L' | 'l' | '<' => Ok(Self::CLeft),
            'U' | 'u' | '^' => Ok(Self::CUp),
            _ => Err(c),
        }
    }
}

impl From<OcarinaNote> for char {
    fn from(note: OcarinaNote) -> Self {
        match note {
            OcarinaNote::A => 'A',
            OcarinaNote::CDown => 'v',
            OcarinaNote::CRight => '>',
            OcarinaNote::CLeft => '<',
            OcarinaNote::CUp => '^',
        }
    }
}