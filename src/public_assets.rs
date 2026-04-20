use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokio::fs;
use tracing::{info, warn};

const TILE_ROUTE_PREFIX: &str = "/assets/public/v1/tiles";

#[derive(Debug, Clone)]
pub struct PublicAssetCatalog {
    version: String,
    generated_at: String,
    packs: Vec<PublicPackSummary>,
    tiles: HashMap<String, PublicTileEntry>,
}

#[derive(Debug, Clone)]
struct PublicTileEntry {
    pack_id: String,
    file_path: PathBuf,
    semantic_group: Option<String>,
    semantic_confidence: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicAssetCatalogResponse {
    pub version: String,
    pub generated_at: String,
    pub pack_count: usize,
    pub tile_count: usize,
    pub packs: Vec<PublicPackSummary>,
    pub tiles: HashMap<String, PublicTileCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicPackSummary {
    pub pack_id: String,
    pub name: String,
    pub status: String,
    pub source_url: String,
    pub import_output_dir: String,
    pub imported_tiles: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicTileCatalogEntry {
    pub pack_id: String,
    pub url: String,
    pub semantic_group: Option<String>,
    pub semantic_confidence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SelectedPacksDoc {
    #[serde(default)]
    version: String,
    #[serde(default)]
    generated_at: String,
    #[serde(default)]
    packs: Vec<SelectedPackEntry>,
}

#[derive(Debug, Deserialize)]
struct SelectedPackEntry {
    pack_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    source_url: String,
    import_output_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PackManifest {
    #[serde(default)]
    assets: Vec<ManifestAssetEntry>,
}

#[derive(Debug, Deserialize)]
struct ManifestAssetEntry {
    asset_id: String,
    file: String,
    semantic_group: Option<String>,
    semantic_confidence: Option<String>,
}

impl PublicAssetCatalog {
    pub fn empty() -> Self {
        Self {
            version: "v1_unavailable".to_string(),
            generated_at: String::new(),
            packs: vec![],
            tiles: HashMap::new(),
        }
    }

    pub async fn load(root_dir: impl AsRef<Path>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let root_dir = if root_dir.as_ref().is_absolute() {
            root_dir.as_ref().to_path_buf()
        } else {
            cwd.join(root_dir)
        };
        let selected_path = root_dir.join("selected_packs.json");
        let selected_raw = match fs::read(&selected_path).await {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!(
                    "public asset catalog unavailable: {} ({e})",
                    selected_path.display()
                );
                return Self::empty();
            }
        };
        let selected: SelectedPacksDoc = match serde_json::from_slice(&selected_raw) {
            Ok(v) => v,
            Err(e) => {
                warn!(
                    "public asset catalog parse failed: {} ({e})",
                    selected_path.display()
                );
                return Self::empty();
            }
        };
        let root_canon = fs::canonicalize(&root_dir)
            .await
            .unwrap_or_else(|_| root_dir.clone());

        let mut packs = Vec::with_capacity(selected.packs.len());
        let mut tiles: HashMap<String, PublicTileEntry> = HashMap::new();
        for pack in selected.packs {
            let mut summary = PublicPackSummary {
                pack_id: pack.pack_id.clone(),
                name: if pack.name.is_empty() {
                    pack.pack_id.clone()
                } else {
                    pack.name
                },
                status: if pack.status.is_empty() {
                    "unknown".to_string()
                } else {
                    pack.status
                },
                source_url: pack.source_url,
                import_output_dir: pack.import_output_dir.clone().unwrap_or_default(),
                imported_tiles: 0,
            };

            let Some(import_output_dir) = pack.import_output_dir else {
                packs.push(summary);
                continue;
            };
            let import_dir = if Path::new(&import_output_dir).is_absolute() {
                PathBuf::from(import_output_dir)
            } else {
                cwd.join(import_output_dir)
            };
            let manifest_path = import_dir.join("manifest.json");
            let manifest_raw = match fs::read(&manifest_path).await {
                Ok(v) => v,
                Err(_) => {
                    packs.push(summary);
                    continue;
                }
            };
            let manifest: PackManifest = match serde_json::from_slice(&manifest_raw) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        "public asset manifest parse failed: {} ({e})",
                        manifest_path.display()
                    );
                    packs.push(summary);
                    continue;
                }
            };
            let mut imported_tiles = 0usize;
            for asset in manifest.assets {
                if !is_png_path(&asset.file) {
                    continue;
                }
                if !is_safe_asset_id(&asset.asset_id) {
                    continue;
                }
                if tiles.contains_key(&asset.asset_id) {
                    warn!(
                        "duplicate public asset id '{}'; keeping first occurrence",
                        asset.asset_id
                    );
                    continue;
                }
                let disk_path = resolve_disk_path(&cwd, &asset.file);
                let disk_canon = match fs::canonicalize(&disk_path).await {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if !disk_canon.starts_with(&root_canon) {
                    warn!(
                        "skipping public asset outside root: {}",
                        disk_canon.display()
                    );
                    continue;
                }
                tiles.insert(
                    asset.asset_id,
                    PublicTileEntry {
                        pack_id: pack.pack_id.clone(),
                        file_path: disk_canon,
                        semantic_group: asset.semantic_group,
                        semantic_confidence: asset.semantic_confidence,
                    },
                );
                imported_tiles += 1;
            }
            summary.imported_tiles = imported_tiles;
            packs.push(summary);
        }

        let catalog = Self {
            version: if selected.version.is_empty() {
                "v1_unknown".to_string()
            } else {
                selected.version
            },
            generated_at: selected.generated_at,
            packs,
            tiles,
        };
        info!(
            "public asset catalog loaded: packs={} tiles={}",
            catalog.packs.len(),
            catalog.tiles.len()
        );
        catalog
    }

    pub fn to_response(&self) -> PublicAssetCatalogResponse {
        let mut tiles = HashMap::with_capacity(self.tiles.len());
        for (asset_id, entry) in &self.tiles {
            tiles.insert(
                asset_id.clone(),
                PublicTileCatalogEntry {
                    pack_id: entry.pack_id.clone(),
                    url: format!("{TILE_ROUTE_PREFIX}/{asset_id}"),
                    semantic_group: entry.semantic_group.clone(),
                    semantic_confidence: entry.semantic_confidence.clone(),
                },
            );
        }
        PublicAssetCatalogResponse {
            version: self.version.clone(),
            generated_at: self.generated_at.clone(),
            pack_count: self.packs.len(),
            tile_count: self.tiles.len(),
            packs: self.packs.clone(),
            tiles,
        }
    }

    pub fn resolve_tile_path(&self, asset_id: &str) -> Option<PathBuf> {
        self.tiles.get(asset_id).map(|v| v.file_path.clone())
    }
}

fn is_png_path(file: &str) -> bool {
    Path::new(file)
        .extension()
        .and_then(|v| v.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
}

fn resolve_disk_path(cwd: &Path, file: &str) -> PathBuf {
    let file_path = PathBuf::from(file);
    if file_path.is_absolute() {
        file_path
    } else {
        cwd.join(file_path)
    }
}

pub fn is_safe_asset_id(asset_id: &str) -> bool {
    !asset_id.is_empty()
        && asset_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs;
    use uuid::Uuid;

    #[tokio::test]
    async fn load_catalog_indexes_pack_tiles() {
        let base = std::env::temp_dir().join(format!("tower_public_assets_{}", Uuid::new_v4()));
        let root = base.join("assets/public/v1");
        let import_dir = root.join("imported/test_pack");
        let tiles_dir = import_dir.join("tiles");
        fs::create_dir_all(&tiles_dir).await.expect("mkdir");

        let tile_file = tiles_dir.join("tile_wall_corner_top_left.png");
        fs::write(&tile_file, b"not_a_real_png")
            .await
            .expect("write tile");
        let outside_file = base.join("outside.png");
        fs::write(&outside_file, b"outside")
            .await
            .expect("write outside");

        let selected = serde_json::json!({
            "version": "v1_test",
            "generated_at": "2026-02-25T00:00:00Z",
            "packs": [{
                "pack_id": "test_pack",
                "name": "Test Pack",
                "status": "imported_v1_stub_ready",
                "source_url": "https://example.invalid/test-pack",
                "import_output_dir": import_dir.to_string_lossy()
            }]
        });
        fs::write(
            root.join("selected_packs.json"),
            serde_json::to_vec_pretty(&selected).expect("selected json"),
        )
        .await
        .expect("write selected");

        let manifest = serde_json::json!({
            "assets": [
                {
                    "asset_id": "tile_wall_corner_top_left",
                    "file": tile_file.to_string_lossy(),
                    "semantic_group": "corner",
                    "semantic_confidence": "high"
                },
                {
                    "asset_id": "tile_wall_outside",
                    "file": outside_file.to_string_lossy()
                }
            ]
        });
        fs::write(
            import_dir.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest json"),
        )
        .await
        .expect("write manifest");

        let catalog = PublicAssetCatalog::load(&root).await;
        let response = catalog.to_response();
        assert_eq!(response.pack_count, 1);
        assert_eq!(response.tile_count, 1);
        let tile = response
            .tiles
            .get("tile_wall_corner_top_left")
            .expect("tile indexed");
        assert_eq!(tile.pack_id, "test_pack");
        assert!(tile.url.ends_with("/tile_wall_corner_top_left"));
        assert!(
            catalog
                .resolve_tile_path("tile_wall_corner_top_left")
                .is_some()
        );
        assert!(catalog.resolve_tile_path("tile_wall_outside").is_none());

        std::fs::remove_dir_all(&base).expect("cleanup temp dir");
    }
}
