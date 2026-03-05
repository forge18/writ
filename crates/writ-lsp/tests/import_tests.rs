use lsp_types::Uri;
use std::io::Write;
use writ_lsp::WorldState;

/// Creates a temp directory with .writ files and opens the main file in the LSP.
///
/// `files` is a list of (relative_path, source) pairs. The last entry is
/// treated as the main file that gets opened in the WorldState.
fn analyze_with_files(files: &[(&str, &str)]) -> (WorldState, Uri, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");

    // Write all files to disk.
    for (path, source) in files {
        let full_path = dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create dirs");
        }
        let mut f = std::fs::File::create(&full_path).expect("failed to create file");
        f.write_all(source.as_bytes())
            .expect("failed to write file");
    }

    // The last file is the main file to analyze.
    let (main_path, _) = files.last().expect("need at least one file");
    let main_full = dir.path().join(main_path);
    let uri_str = format!("file://{}", main_full.display());
    let uri: Uri = uri_str.parse().unwrap();

    let mut world = WorldState::new();
    let source = std::fs::read_to_string(&main_full).unwrap();
    world.open_document(uri.clone(), source);

    (world, uri, dir)
}

#[test]
fn test_named_import_from_disk() {
    let weapon_src = concat!(
        "export func getHealth() -> int {\n",
        "    return 100\n",
        "}\n",
    );
    let main_src = concat!(
        "import { getHealth } from \"weapon\"\n",
        "let h: int = getHealth()\n",
    );

    let (world, uri, _dir) =
        analyze_with_files(&[("weapon.writ", weapon_src), ("main.writ", main_src)]);

    let doc = world.get_document(&uri).unwrap();

    // Should have no "unknown module" diagnostics.
    let import_errors: Vec<_> = doc
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("unknown module"))
        .collect();
    assert!(
        import_errors.is_empty(),
        "should resolve import from disk, got errors: {import_errors:?}"
    );
}

#[test]
fn test_named_import_nested_path() {
    let weapon_src = concat!(
        "export class Weapon {\n",
        "    public damage: float = 10.0\n",
        "}\n",
    );
    let main_src = concat!(
        "import { Weapon } from \"items/weapon\"\n",
        "let w = Weapon()\n",
    );

    let (world, uri, _dir) =
        analyze_with_files(&[("items/weapon.writ", weapon_src), ("main.writ", main_src)]);

    let doc = world.get_document(&uri).unwrap();

    let import_errors: Vec<_> = doc
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("unknown module"))
        .collect();
    assert!(
        import_errors.is_empty(),
        "should resolve nested import path from disk, got errors: {import_errors:?}"
    );
}

#[test]
fn test_wildcard_import_from_disk() {
    let enemy_src = concat!(
        "export func getHealth() -> int {\n",
        "    return 50\n",
        "}\n",
    );
    let main_src = concat!(
        "import * as enemy from \"enemy\"\n",
        "let h: int = enemy::getHealth()\n",
    );

    let (world, uri, _dir) =
        analyze_with_files(&[("enemy.writ", enemy_src), ("main.writ", main_src)]);

    let doc = world.get_document(&uri).unwrap();

    let import_errors: Vec<_> = doc
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("unknown module") || d.message.contains("unknown namespace"))
        .collect();
    assert!(
        import_errors.is_empty(),
        "should resolve wildcard import from disk, got errors: {import_errors:?}"
    );
}

#[test]
fn test_missing_import_still_produces_diagnostic() {
    // When the imported file doesn't exist on disk, we should still get
    // the "unknown module" diagnostic.
    let main_src = "import { Weapon } from \"nonexistent\"\n";

    let (world, uri, _dir) = analyze_with_files(&[("main.writ", main_src)]);

    let doc = world.get_document(&uri).unwrap();

    let has_error = doc
        .diagnostics
        .iter()
        .any(|d| d.message.contains("unknown module"));
    assert!(
        has_error,
        "should produce 'unknown module' diagnostic for missing file"
    );
}
