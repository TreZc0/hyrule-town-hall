use {
    std::{
        collections::HashMap,
        env,
        fs::{
            self,
            File,
        },
        io::prelude::*,
        path::{
            Path,
            PathBuf,
        },
    },
    git2::{
        Oid,
        Repository,
    },
    semver::Version,
};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)] Git(#[from] git2::Error),
    #[error(transparent)] Io(#[from] std::io::Error),
}

fn check_static_file(cache: &mut HashMap<PathBuf, Oid>, repo: &Repository, relative_path: &Path, path: PathBuf) -> Result<(), Error> {
    let mut iter_commit = repo.head()?.peel_to_commit()?;
    let commit_id = loop {
        let iter_commit_id = iter_commit.id();
        if iter_commit.parent_count() != 1 {
            // initial commit or merge commit; mark the file as updated here for simplicity's sake
            break iter_commit_id
        }
        let parent = iter_commit.parent(0)?;
        let diff = repo.diff_tree_to_tree(Some(&parent.tree()?), Some(&iter_commit.tree()?), Some(git2::DiffOptions::default().pathspec(&path)))?;
        if diff.deltas().next().is_some() {
            break iter_commit_id
        }
        iter_commit = parent;
    };
    cache.insert(relative_path.to_owned(), commit_id);
    Ok(())
}

fn check_static_dir(cache: &mut HashMap<PathBuf, Oid>, repo: &Repository, relative_path: &Path, path: PathBuf) -> Result<(), Error> {
    for entry in fs::read_dir(&path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            // Skip hash-icon and hash-icon-500 directories as they contain many unused files
            let file_name = entry.file_name();
            if file_name == "hash-icon" || file_name == "hash-icon-500" {
                continue;
            }
            check_static_dir(cache, repo, &relative_path.join(entry.file_name()), entry.path())?;
        } else {
            check_static_file(cache, repo, &relative_path.join(entry.file_name()), entry.path())?;
        }
    }
    Ok(())
}

fn main() -> Result<(), Error> {
    println!("cargo:rerun-if-changed=nonexistent.foo");
    let static_dir = Path::new("assets").join("static");
    let mut cache = HashMap::default();
    let repo = Repository::open(&env::var_os("CARGO_MANIFEST_DIR").unwrap())?;
    for entry in fs::read_dir(&static_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            check_static_dir(&mut cache, &repo, entry.file_name().as_ref(), entry.path())?;
        } else {
            check_static_file(&mut cache, &repo, entry.file_name().as_ref(), entry.path())?;
        }
    }
    let mut out_f = File::create(Path::new(&env::var_os("OUT_DIR").unwrap()).join("static_files.rs"))?;
    // Split entries into icon and non-icon
    let mut icon_entries = Vec::new();
    let mut normal_entries = Vec::new();
    for (path, commit_id) in cache {
        let unix_path = path.to_str().expect("non-UTF-8 static file path").replace('\\', "/");
        if unix_path.starts_with("hash-icon/") || unix_path.starts_with("hash-icon-500/") {
            icon_entries.push((unix_path, commit_id));
        } else {
            normal_entries.push((unix_path, commit_id));
        }
    }
    // Write normal static_url! macro
    writeln!(&mut out_f, "macro_rules! static_url {{")?;
    for (unix_path, commit_id) in &normal_entries {
        let uri = format!("/static/{unix_path}?v={commit_id}");
        writeln!(&mut out_f, "    ({unix_path:?}) => {{")?;
        writeln!(&mut out_f, "        ::rocket_util::Origin(::rocket::uri!({uri:?}))")?;
        writeln!(&mut out_f, "    }};")?;
    }
    writeln!(&mut out_f, "}}")?;
    // Write icon static_url_icon! macro with allow unused
    writeln!(&mut out_f, "#[allow(unused_macros, unused_macro_rules)]")?;
    writeln!(&mut out_f, "macro_rules! static_url_icon {{")?;
    for (unix_path, commit_id) in &icon_entries {
        let uri = format!("/static/{unix_path}?v={commit_id}");
        writeln!(&mut out_f, "    ({unix_path:?}) => {{")?;
        writeln!(&mut out_f, "        ::rocket_util::Origin(::rocket::uri!({uri:?}))")?;
        writeln!(&mut out_f, "    }};")?;
    }
    writeln!(&mut out_f, "}}")?;
    let mut out_f = File::create(Path::new(&env::var_os("OUT_DIR").unwrap()).join("version.rs"))?;
    let version = env!("CARGO_PKG_VERSION").parse::<Version>().unwrap();
    assert!(version.pre.is_empty());
    assert!(version.build.is_empty());
    let commit_hash = repo.head().unwrap().peel_to_commit().unwrap().id();
    writeln!(&mut out_f, "pub const CLAP_VERSION: &str = {:?};", format!("{version} ({commit_hash})")).unwrap();
    Ok(())
}
