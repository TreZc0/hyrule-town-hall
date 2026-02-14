#![allow(unused_qualifications)] // in derive macro

use crate::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Sequence, Deserialize, sqlx::Type, async_graphql::Enum, FromFormField)]
#[sqlx(type_name = "language")]
pub(crate) enum Language {
    #[serde(rename = "en", alias = "English")]
    #[sqlx(rename = "en")]
    #[field(value = "en")]
    English,
    #[serde(rename = "fr", alias = "French")]
    #[sqlx(rename = "fr")]
    #[field(value = "fr")]
    French,
    #[serde(rename = "de", alias = "German")]
    #[sqlx(rename = "de")]
    #[field(value = "de")]
    German,
    #[serde(rename = "pt", alias = "Portuguese")]
    #[sqlx(rename = "pt")]
    #[field(value = "pt")]
    Portuguese,
}

impl Language {
    pub(crate) fn short_code(&self) -> &'static str {
        match self {
            English => "en",
            French => "fr",
            German => "de",
            Portuguese => "pt",
        }
    }

    pub(crate) fn format_duration(&self, duration: Duration, running_text: bool) -> String {
        let secs = duration.as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;
        if running_text {
            match self {
                French => {
                    let parts = (hours > 0).then(|| format!("{hours} heure{}", if hours == 1 { "" } else { "s" })).into_iter()
                        .chain((mins > 0).then(|| format!("{mins} minute{}", if mins == 1 { "" } else { "s" })))
                        .chain((secs > 0).then(|| format!("{secs} seconde{}", if secs == 1 { "" } else { "s" })));
                    French.join_str_opt(parts).unwrap_or_else(|| format!("0 secondes"))
                }
                _ => {
                    let parts = (hours > 0).then(|| format!("{hours} hour{}", if hours == 1 { "" } else { "s" })).into_iter()
                        .chain((mins > 0).then(|| format!("{mins} minute{}", if mins == 1 { "" } else { "s" })))
                        .chain((secs > 0).then(|| format!("{secs} second{}", if secs == 1 { "" } else { "s" })));
                    English.join_str_opt(parts).unwrap_or_else(|| format!("0 seconds"))
                }
            }
        } else {
            format!("{hours}:{mins:02}:{secs:02}")
        }
    }

    pub(crate) fn join_html<T: ToHtml>(&self, elts: impl IntoNonEmptyIterator<Item = T>) -> RawHtml<String> {
        match self {
            French | German | Portuguese => {
                let (first, rest) = elts.into_nonempty_iter().next();
                let mut rest = rest.fuse();
                if let Some(second) = rest.next() {
                    let mut rest = iter::once(second).chain(rest).collect_vec();
                    let last = rest.pop().expect("rest contains at least second");
                    html! {
                        : first;
                        @for elt in rest {
                            : ", ";
                            : elt;
                        }
                        @match self {
                            French => : " et ";
                            German => : " und ";
                            Portuguese => : " e ";
                            _ => @unreachable
                        }
                        : last;
                    }
                } else {
                    first.to_html()
                }
            }
            _ => {
                let (first, rest) = elts.into_nonempty_iter().next();
                let mut rest = rest.fuse();
                match (rest.next(), rest.next()) {
                    (None, _) => first.to_html(),
                    (Some(second), None) => html! {
                        : first;
                        : " and ";
                        : second;
                    },
                    (Some(second), Some(third)) => {
                        let mut rest = [second, third].into_nonempty_iter().chain(rest).collect::<NEVec<_>>();
                        let last = rest.pop().expect("rest contains at least second and third");
                        html! {
                            : first;
                            @for elt in rest {
                                : ", ";
                                : elt;
                            }
                            : ", and ";
                            : last;
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn join_html_opt<T: ToHtml>(&self, elts: impl IntoIterator<Item = T>) -> Option<RawHtml<String>> {
        elts.try_into_nonempty_iter().map(|iter| self.join_html(iter))
    }

    pub(crate) fn join_str<T: fmt::Display>(&self, elts: impl IntoNonEmptyIterator<Item = T>) -> String {
        match self {
            French => French.join_str_with("et", elts),
            German => German.join_str_with("und", elts),
            Portuguese => Portuguese.join_str_with("e", elts),
            _ => English.join_str_with("and", elts),
        }
    }

    pub(crate) fn join_str_opt<T: fmt::Display>(&self, elts: impl IntoIterator<Item = T>) -> Option<String> {
        elts.try_into_nonempty_iter().map(|iter| self.join_str(iter))
    }

    pub(crate) fn join_str_with<T: fmt::Display>(&self, conjunction: &str, elts: impl IntoNonEmptyIterator<Item = T>) -> String {
        match self {
            French | German | Portuguese => {
                let (first, rest) = elts.into_nonempty_iter().next();
                let mut rest = rest.fuse();
                if let Some(second) = rest.next() {
                    let mut rest = iter::once(second).chain(rest).collect_vec();
                    let last = rest.pop().expect("rest contains at least second");
                    format!("{first}{} {conjunction} {last}", rest.into_iter().map(|elt| format!(", {elt}")).format(""))
                } else {
                    first.to_string()
                }
            }
            _ => {
                let (first, rest) = elts.into_nonempty_iter().next();
                let mut rest = rest.fuse();
                match (rest.next(), rest.next()) {
                    (None, _) => first.to_string(),
                    (Some(second), None) => format!("{first} {conjunction} {second}"),
                    (Some(second), Some(third)) => {
                        let mut rest = [second, third].into_nonempty_iter().chain(rest).collect::<NEVec<_>>();
                        let last = rest.pop().expect("rest contains at least second and third");
                        format!("{first}, {}, {conjunction} {last}", rest.into_iter().format(", "))
                    }
                }
            }
        }
    }

    pub(crate) fn join_str_opt_with<T: fmt::Display>(&self, conjunction: &str, elts: impl IntoIterator<Item = T>) -> Option<String> {
        elts.try_into_nonempty_iter().map(|iter| self.join_str_with(conjunction, iter))
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            English => write!(f, "English"),
            French => write!(f, "French"),
            German => write!(f, "German"),
            Portuguese => write!(f, "Portuguese"),
        }
    }
}

impl ToHtml for Language {
    fn to_html(&self) -> RawHtml<String> {
        html! {
            : self.to_string();
        }
    }

    fn push_html(&self, buf: &mut RawHtml<String>) {
        match self {
            English => write!(&mut buf.0, "English"),
            French => write!(&mut buf.0, "French"),
            German => write!(&mut buf.0, "German"),
            Portuguese => write!(&mut buf.0, "Portuguese"),
        }.unwrap();
    }
}

pub(crate) fn english_ordinal(n: usize) -> String {
    match n % 10 {
        1 if n % 100 != 11 => format!("{n}st"),
        2 if n % 100 != 12 => format!("{n}nd"),
        3 if n % 100 != 13 => format!("{n}rd"),
        _ => format!("{n}th"),
    }
}
