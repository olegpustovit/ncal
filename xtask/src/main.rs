use std::path::{Path, PathBuf};
use std::{env, fs, process};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must live one level below workspace root")
        .to_path_buf()
}

fn read_cargo_doc() -> (PathBuf, toml_edit::DocumentMut) {
    let path = workspace_root().join("Cargo.toml");
    let src = fs::read_to_string(&path).expect("read Cargo.toml");
    let doc: toml_edit::DocumentMut = src.parse().expect("parse Cargo.toml");
    (path, doc)
}

fn workspace_version(doc: &toml_edit::DocumentMut) -> semver::Version {
    doc.get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .expect("workspace.package.version missing")
        .parse()
        .expect("invalid workspace.package.version")
}

fn update_readme_version_badge(old: &semver::Version, new: &semver::Version) {
    let readme = workspace_root().join("README.md");
    let Ok(text) = fs::read_to_string(&readme) else {
        return;
    };
    let updated = text.replace(&format!("version-v{old}-"), &format!("version-v{new}-"));
    if updated != text {
        fs::write(&readme, updated).expect("write README.md");
    }
}

fn bump_version(kind: &str) {
    let (path, mut doc) = read_cargo_doc();
    let old = workspace_version(&doc);
    let new = match kind {
        "minor" => semver::Version::new(old.major, old.minor + 1, 0),
        "patch" => semver::Version::new(old.major, old.minor, old.patch + 1),
        other => {
            eprintln!("error: expected `minor` or `patch`, got `{other}`");
            process::exit(2);
        }
    };

    let version = doc["workspace"]["package"]
        .get_mut("version")
        .expect("workspace.package.version missing");
    *version = toml_edit::value(new.to_string());
    fs::write(&path, doc.to_string()).expect("write Cargo.toml");
    update_readme_version_badge(&old, &new);
    println!("{new}");
}

fn read_version() {
    let (_path, doc) = read_cargo_doc();
    println!("{}", workspace_version(&doc));
}

fn check_boundary() -> Result<(), String> {
    let root = workspace_root();
    for package in ["ncal-api", "ncal-core", "ncal-mcp"] {
        let tree = process::Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&root)
            .args([
                "tree",
                "-p",
                package,
                "--no-default-features",
                "--edges",
                "normal",
                "--prefix",
                "depth",
                "--format",
                "{p}",
                "--no-dedupe",
            ])
            .output()
            .map_err(|error| format!("run cargo tree: {error}"))?;
        if !tree.status.success() {
            return Err(String::from_utf8_lossy(&tree.stderr).into_owned());
        }
        let output = String::from_utf8(tree.stdout).map_err(|error| error.to_string())?;
        let mut ancestors = Vec::new();
        for line in output.lines() {
            let depth_len = line.bytes().take_while(u8::is_ascii_digit).count();
            let depth = line[..depth_len]
                .parse::<usize>()
                .map_err(|error| error.to_string())?;
            let name = line[depth_len..]
                .split_whitespace()
                .next()
                .ok_or("missing crate name")?;
            ancestors.truncate(depth);
            // Reqwest requires this client middleware even with default features disabled.
            let allowed_client_edge = name == "tower-http" && ancestors.last() == Some(&"reqwest");
            if !allowed_client_edge
                && matches!(
                    name,
                    "axum" | "sqlx" | "idclient" | "hyper-server" | "tower-http"
                )
            {
                return Err(format!("{package}: forbidden dependency {name}"));
            }
            ancestors.push(name);
        }
    }
    check_sources(&root.join("crates"))
}

fn check_sources(dir: &Path) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            check_sources(&path)?;
        } else if path.extension().is_some_and(|extension| extension == "rs")
            && fs::read_to_string(&path)
                .map_err(|error| error.to_string())?
                .contains("microapps.space")
        {
            return Err(format!("hosted hostname in {}", path.display()));
        }
    }
    Ok(())
}

fn usage() -> ! {
    eprintln!(
        "usage: cargo xtask <task>\n\n\
         tasks:\n\
           bump-version minor|patch  Bump workspace semver\n\
           read-version              Print current workspace version\n\
           check-boundary            Verify library dependency and source boundaries"
    );
    process::exit(2)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("bump-version") if args.len() == 3 => bump_version(&args[2]),
        Some("bump-version") => usage(),
        Some("read-version") if args.len() == 2 => read_version(),
        Some("read-version") => usage(),
        Some("check-boundary") if args.len() == 2 => {
            if let Err(error) = check_boundary() {
                eprintln!("error: {error}");
                process::exit(1);
            }
        }
        Some(other) => {
            eprintln!("unknown task: {other}");
            usage();
        }
        None => usage(),
    }
}
