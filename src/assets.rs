use base64::Engine;
use chrono::{DateTime, Utc};
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::{
    fs,
    process::Command,
    time::{Duration, sleep},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    AgentSprite,
    DepartmentProp,
    EnvironmentTile,
    UiOverlay,
    Fx,
    Portrait,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSpec {
    pub tower_id: String,
    pub floor_id: String,
    pub department_id: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub width: u16,
    pub height: u16,
    pub frames: u16,
    pub tags: Vec<String>,
    pub palette_id: String,
    pub prompt: String,
    pub preferred_lane: Option<GenerationLane>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationLane {
    Scenario,
    OpenAiImages,
    Replicate,
    ComfyUi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetManifest {
    pub asset_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub tower_id: String,
    pub floor_id: String,
    pub department_id: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub generation_lane: GenerationLane,
    pub prompt: String,
    pub prompt_hash: String,
    pub palette_id: String,
    pub width: u16,
    pub height: u16,
    pub frames: u16,
    pub status: String,
    pub generator_status: String,
    pub files: ManifestFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFiles {
    pub spec_json: String,
    pub raw_output: String,
    pub generated_image: Option<String>,
    pub normalized_output: String,
    pub sprite_sheet: String,
    pub metadata_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateAssetRequest {
    pub spec: AssetSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateAssetResponse {
    pub manifest: AssetManifest,
}

#[derive(Debug, Clone)]
pub struct AssetService {
    base_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutingStrategy {
    OpenAiOnly,
    ClassRouter,
}

#[derive(Debug, Clone)]
struct LaneDecision {
    lane: GenerationLane,
    strategy: RoutingStrategy,
    requested_lane: Option<GenerationLane>,
    forced_by_strategy: bool,
}

#[derive(Debug, Clone, Copy)]
struct ClassRule {
    width: &'static [u16],
    height: &'static [u16],
    min_frames: u16,
    max_frames: u16,
}

#[derive(Debug, Clone, Copy)]
struct PaletteProfile {
    id: &'static str,
    colors: &'static [&'static str],
    transparent_index: u8,
    max_colors: u8,
    style_version: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct PlaceholderTemplate {
    key: &'static str,
    glyph: char,
    summary: &'static str,
}

const LOW_FI_STYLE_VERSION: &str = "lowfi_v1";
const TOWER_V1_COLORS: [&str; 16] = [
    "#0c0f14", "#1f2a44", "#2e4f8a", "#4c7ed9", "#9ccbff", "#1d3b2a", "#2e6b3f", "#5da85a",
    "#a2d36e", "#5a3a22", "#8b5a2b", "#c98b3a", "#dad0a6", "#8a8f99", "#c9d1d9", "#f4f7fa",
];
const TOWER_V1_PALETTE: PaletteProfile = PaletteProfile {
    id: "tower_v1",
    colors: &TOWER_V1_COLORS,
    transparent_index: 0,
    max_colors: 16,
    style_version: LOW_FI_STYLE_VERSION,
};

impl AssetService {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    pub async fn generate(
        &self,
        req: GenerateAssetRequest,
    ) -> Result<GenerateAssetResponse, String> {
        let palette = validate_spec(&req.spec)?;
        let lane_decision = resolve_lane(&req.spec);
        let lane = lane_decision.lane.clone();
        let asset_id = Uuid::new_v4();
        let asset_dir = self.base_dir.join(asset_id.to_string());
        fs::create_dir_all(&asset_dir)
            .await
            .map_err(|e| format!("create dir: {e}"))?;

        let spec_path = asset_dir.join("spec.json");
        let raw_path = asset_dir.join("raw_output.txt");
        let generated_image_path = asset_dir.join("raw_output_image.bin");
        let normalized_path = asset_dir.join("normalized_output.txt");
        let sprite_sheet_path = asset_dir.join("sprite_sheet.txt");
        let metadata_path = asset_dir.join("metadata.json");
        let prompt_hash = format!("{:x}", md5::compute(req.spec.prompt.as_bytes()));

        let spec_json =
            serde_json::to_vec_pretty(&req.spec).map_err(|e| format!("spec json: {e}"))?;
        fs::write(&spec_path, spec_json)
            .await
            .map_err(|e| format!("write spec: {e}"))?;

        let (generator_status, generated_image_file, used_placeholder) =
            simulate_or_call_generator(&lane, &req.spec, &raw_path, &generated_image_path).await?;
        normalize_with_aseprite_or_fallback(&raw_path, &normalized_path, palette).await?;
        build_sprite_sheet_stub(&normalized_path, &sprite_sheet_path, &req.spec, palette).await?;
        build_metadata_stub(
            &metadata_path,
            &req.spec,
            &lane,
            &prompt_hash,
            &lane_decision,
            palette,
            used_placeholder,
        )
        .await?;

        let manifest = AssetManifest {
            asset_id,
            created_at: Utc::now(),
            tower_id: req.spec.tower_id.clone(),
            floor_id: req.spec.floor_id.clone(),
            department_id: req.spec.department_id.clone(),
            name: req.spec.name.clone(),
            asset_class: req.spec.asset_class,
            generation_lane: lane,
            prompt: req.spec.prompt.clone(),
            prompt_hash,
            palette_id: req.spec.palette_id.clone(),
            width: req.spec.width,
            height: req.spec.height,
            frames: req.spec.frames,
            status: if used_placeholder {
                "placeholder_ready".to_string()
            } else {
                "ready".to_string()
            },
            generator_status,
            files: ManifestFiles {
                spec_json: spec_path.to_string_lossy().to_string(),
                raw_output: raw_path.to_string_lossy().to_string(),
                generated_image: generated_image_file
                    .map(|_| generated_image_path.to_string_lossy().to_string()),
                normalized_output: normalized_path.to_string_lossy().to_string(),
                sprite_sheet: sprite_sheet_path.to_string_lossy().to_string(),
                metadata_json: metadata_path.to_string_lossy().to_string(),
            },
        };

        let manifest_path = asset_dir.join("manifest.json");
        let manifest_json =
            serde_json::to_vec_pretty(&manifest).map_err(|e| format!("manifest json: {e}"))?;
        fs::write(&manifest_path, manifest_json)
            .await
            .map_err(|e| format!("write manifest: {e}"))?;

        Ok(GenerateAssetResponse { manifest })
    }

    pub async fn get_manifest(&self, asset_id: Uuid) -> Result<AssetManifest, String> {
        let path = self
            .base_dir
            .join(asset_id.to_string())
            .join("manifest.json");
        let bytes = fs::read(path)
            .await
            .map_err(|e| format!("read manifest: {e}"))?;
        serde_json::from_slice::<AssetManifest>(&bytes).map_err(|e| format!("parse manifest: {e}"))
    }
}

fn route_lane(class: &AssetClass) -> GenerationLane {
    match class {
        AssetClass::AgentSprite | AssetClass::Portrait => GenerationLane::Scenario,
        AssetClass::DepartmentProp | AssetClass::EnvironmentTile => GenerationLane::Replicate,
        AssetClass::UiOverlay | AssetClass::Fx => GenerationLane::OpenAiImages,
    }
}

fn parse_routing_strategy(raw: &str) -> RoutingStrategy {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "class_router" => RoutingStrategy::ClassRouter,
        _ => RoutingStrategy::OpenAiOnly,
    }
}

fn configured_routing_strategy() -> RoutingStrategy {
    let raw = std::env::var("ASSET_ROUTING_MODE").unwrap_or_else(|_| "openai_only".to_string());
    parse_routing_strategy(&raw)
}

fn resolve_lane_with_strategy(spec: &AssetSpec, strategy: RoutingStrategy) -> LaneDecision {
    let requested_lane = spec.preferred_lane.clone();
    match strategy {
        RoutingStrategy::OpenAiOnly => {
            let forced_by_strategy = requested_lane
                .as_ref()
                .is_some_and(|lane| lane != &GenerationLane::OpenAiImages);
            LaneDecision {
                lane: GenerationLane::OpenAiImages,
                strategy,
                requested_lane,
                forced_by_strategy,
            }
        }
        RoutingStrategy::ClassRouter => {
            let lane = requested_lane
                .clone()
                .unwrap_or_else(|| route_lane(&spec.asset_class));
            LaneDecision {
                lane,
                strategy,
                requested_lane,
                forced_by_strategy: false,
            }
        }
    }
}

fn resolve_lane(spec: &AssetSpec) -> LaneDecision {
    resolve_lane_with_strategy(spec, configured_routing_strategy())
}

fn validate_spec(spec: &AssetSpec) -> Result<&'static PaletteProfile, String> {
    if spec.width == 0 || spec.height == 0 {
        return Err("width/height must be > 0".to_string());
    }
    if spec.frames == 0 {
        return Err("frames must be > 0".to_string());
    }
    if spec.name.trim().is_empty() {
        return Err("name required".to_string());
    }
    if !spec
        .name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err("name must use [a-z0-9_]".to_string());
    }
    if spec.tags.is_empty() {
        return Err("at least one tag required".to_string());
    }
    if spec.prompt.trim().is_empty() {
        return Err("prompt required".to_string());
    }
    let palette_id = spec.palette_id.trim();
    let palette = palette_profile(palette_id)
        .ok_or_else(|| format!("unsupported palette_id '{palette_id}'"))?;
    let rule = class_rule(&spec.asset_class);
    if !rule.width.contains(&spec.width) {
        return Err(format!(
            "invalid width {} for {:?}; allowed: {}",
            spec.width,
            spec.asset_class,
            join_u16(rule.width)
        ));
    }
    if !rule.height.contains(&spec.height) {
        return Err(format!(
            "invalid height {} for {:?}; allowed: {}",
            spec.height,
            spec.asset_class,
            join_u16(rule.height)
        ));
    }
    if spec.frames < rule.min_frames || spec.frames > rule.max_frames {
        return Err(format!(
            "invalid frames {} for {:?}; allowed range: {}-{}",
            spec.frames, spec.asset_class, rule.min_frames, rule.max_frames
        ));
    }
    Ok(palette)
}

fn class_rule(class: &AssetClass) -> ClassRule {
    match class {
        AssetClass::AgentSprite => ClassRule {
            width: &[32],
            height: &[32],
            min_frames: 4,
            max_frames: 8,
        },
        AssetClass::DepartmentProp => ClassRule {
            width: &[32],
            height: &[32],
            min_frames: 1,
            max_frames: 6,
        },
        AssetClass::EnvironmentTile => ClassRule {
            width: &[16, 32],
            height: &[16, 32],
            min_frames: 1,
            max_frames: 4,
        },
        AssetClass::UiOverlay => ClassRule {
            width: &[16, 32],
            height: &[16, 32],
            min_frames: 1,
            max_frames: 4,
        },
        AssetClass::Fx => ClassRule {
            width: &[16, 32],
            height: &[16, 32],
            min_frames: 2,
            max_frames: 8,
        },
        AssetClass::Portrait => ClassRule {
            width: &[48],
            height: &[48],
            min_frames: 1,
            max_frames: 5,
        },
    }
}

fn join_u16(values: &[u16]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn palette_profile(id: &str) -> Option<&'static PaletteProfile> {
    match id {
        "tower_v1" => Some(&TOWER_V1_PALETTE),
        _ => None,
    }
}

fn palette_checksum_hex(palette: &PaletteProfile) -> String {
    let joined = palette.colors.join(",");
    format!("{:x}", md5::compute(joined.as_bytes()))
}

async fn simulate_or_call_generator(
    lane: &GenerationLane,
    spec: &AssetSpec,
    output_path: &Path,
    generated_image_path: &Path,
) -> Result<(String, Option<()>, bool), String> {
    match lane {
        GenerationLane::Scenario => {
            if std::env::var("SCENARIO_API_KEY").is_ok()
                && std::env::var("SCENARIO_API_SECRET").is_ok()
            {
                match call_scenario_txt2img(spec, output_path, generated_image_path).await {
                    Ok(true) => Ok(("scenario_api_success".to_string(), Some(()), false)),
                    Ok(false) => {
                        let status = "scenario_api_success_no_image_url".to_string();
                        write_placeholder_output(
                            output_path,
                            lane,
                            spec,
                            &status,
                            "provider_success_no_image_url",
                        )
                        .await?;
                        Ok((status, None, true))
                    }
                    Err(e) => {
                        let status = "scenario_api_error_fallback".to_string();
                        write_placeholder_output(
                            output_path,
                            lane,
                            spec,
                            &status,
                            &format!("provider_error:{e}"),
                        )
                        .await?;
                        Ok((status, None, true))
                    }
                }
            } else {
                let status = "scenario_not_configured_placeholder".to_string();
                write_placeholder_output(
                    output_path,
                    lane,
                    spec,
                    &status,
                    "provider_not_configured",
                )
                .await?;
                Ok((status, None, true))
            }
        }
        GenerationLane::OpenAiImages => {
            if std::env::var("OPENAI_API_KEY").is_ok() {
                match call_openai_images(spec, output_path, generated_image_path).await {
                    Ok(true) => Ok(("openai_api_success".to_string(), Some(()), false)),
                    Ok(false) => {
                        let status = "openai_api_success_no_image_url".to_string();
                        write_placeholder_output(
                            output_path,
                            lane,
                            spec,
                            &status,
                            "provider_success_no_image_url",
                        )
                        .await?;
                        Ok((status, None, true))
                    }
                    Err(e) => {
                        let status = "openai_api_error_fallback".to_string();
                        write_placeholder_output(
                            output_path,
                            lane,
                            spec,
                            &status,
                            &format!("provider_error:{e}"),
                        )
                        .await?;
                        Ok((status, None, true))
                    }
                }
            } else {
                let status = "openai_not_configured_placeholder".to_string();
                write_placeholder_output(
                    output_path,
                    lane,
                    spec,
                    &status,
                    "provider_not_configured",
                )
                .await?;
                Ok((status, None, true))
            }
        }
        GenerationLane::Replicate => {
            if std::env::var("REPLICATE_API_TOKEN").is_ok() {
                match call_replicate_prediction(spec, output_path, generated_image_path).await {
                    Ok(true) => Ok(("replicate_api_success".to_string(), Some(()), false)),
                    Ok(false) => {
                        let status = "replicate_api_success_no_image_url".to_string();
                        write_placeholder_output(
                            output_path,
                            lane,
                            spec,
                            &status,
                            "provider_success_no_image_url",
                        )
                        .await?;
                        Ok((status, None, true))
                    }
                    Err(e) => {
                        let status = "replicate_api_error_fallback".to_string();
                        write_placeholder_output(
                            output_path,
                            lane,
                            spec,
                            &status,
                            &format!("provider_error:{e}"),
                        )
                        .await?;
                        Ok((status, None, true))
                    }
                }
            } else {
                let status = "replicate_not_configured_placeholder".to_string();
                write_placeholder_output(
                    output_path,
                    lane,
                    spec,
                    &status,
                    "provider_not_configured",
                )
                .await?;
                Ok((status, None, true))
            }
        }
        GenerationLane::ComfyUi => {
            if std::env::var("COMFY_API_URL").is_ok()
                && std::env::var("COMFY_WORKFLOW_FILE").is_ok()
            {
                match call_comfyui_workflow(spec, output_path, generated_image_path).await {
                    Ok(true) => Ok(("comfy_api_success".to_string(), Some(()), false)),
                    Ok(false) => {
                        let status = "comfy_api_success_no_image_url".to_string();
                        write_placeholder_output(
                            output_path,
                            lane,
                            spec,
                            &status,
                            "provider_success_no_image_url",
                        )
                        .await?;
                        Ok((status, None, true))
                    }
                    Err(e) => {
                        let status = "comfy_api_error_fallback".to_string();
                        write_placeholder_output(
                            output_path,
                            lane,
                            spec,
                            &status,
                            &format!("provider_error:{e}"),
                        )
                        .await?;
                        Ok((status, None, true))
                    }
                }
            } else {
                let status = "comfy_not_configured_placeholder".to_string();
                write_placeholder_output(
                    output_path,
                    lane,
                    spec,
                    &status,
                    "provider_not_configured",
                )
                .await?;
                Ok((status, None, true))
            }
        }
    }
}

fn placeholder_template(class: &AssetClass) -> PlaceholderTemplate {
    match class {
        AssetClass::AgentSprite => PlaceholderTemplate {
            key: "agent_sprite",
            glyph: '@',
            summary: "humanoid-agent",
        },
        AssetClass::DepartmentProp => PlaceholderTemplate {
            key: "department_prop",
            glyph: '#',
            summary: "department-prop",
        },
        AssetClass::EnvironmentTile => PlaceholderTemplate {
            key: "environment_tile",
            glyph: '=',
            summary: "environment-tile",
        },
        AssetClass::UiOverlay => PlaceholderTemplate {
            key: "ui_overlay",
            glyph: '+',
            summary: "ui-overlay",
        },
        AssetClass::Fx => PlaceholderTemplate {
            key: "fx",
            glyph: '*',
            summary: "effect-burst",
        },
        AssetClass::Portrait => PlaceholderTemplate {
            key: "portrait",
            glyph: '&',
            summary: "portrait-face",
        },
    }
}

fn placeholder_preview(width: u16, height: u16, glyph: char) -> String {
    let rows = height.clamp(2, 8);
    let cols = width.clamp(2, 8);
    let mut out = String::new();
    for _ in 0..rows {
        out.push_str(&glyph.to_string().repeat(cols as usize));
        out.push('\n');
    }
    out
}

async fn write_placeholder_output(
    output_path: &Path,
    lane: &GenerationLane,
    spec: &AssetSpec,
    status: &str,
    reason: &str,
) -> Result<(), String> {
    let template = placeholder_template(&spec.asset_class);
    let preview = placeholder_preview(spec.width, spec.height, template.glyph);
    let payload = format!(
        "fallback=true\nfallback_status={status}\nfallback_reason={reason}\nplaceholder_key={}\nplaceholder_summary={}\nlane={:?}\nname={}\nasset_class={:?}\nsize={}x{}\nframes={}\nprompt={}\npreview:\n{}",
        template.key,
        template.summary,
        lane,
        spec.name,
        spec.asset_class,
        spec.width,
        spec.height,
        spec.frames,
        spec.prompt,
        preview
    );
    fs::write(output_path, payload)
        .await
        .map_err(|e| format!("write placeholder raw output: {e}"))?;
    Ok(())
}

async fn normalize_with_aseprite_or_fallback(
    input: &Path,
    output: &Path,
    palette: &PaletteProfile,
) -> Result<(), String> {
    let script = PathBuf::from("scripts/aseprite_normalize.sh");
    if script.exists() {
        let status = Command::new("bash")
            .arg(script)
            .arg(input)
            .arg(output)
            .arg(palette.id)
            .status()
            .await
            .map_err(|e| format!("normalize script failed to run: {e}"))?;
        if status.success() {
            return Ok(());
        }
    }
    let source = fs::read_to_string(input)
        .await
        .map_err(|e| format!("fallback normalize read: {e}"))?;
    let normalized = format!(
        "NORMALIZED_V1\npalette_id={}\npalette_checksum={}\ntransparent_index={}\nmax_colors={}\nstyle_version={}\nsource:\n{}",
        palette.id,
        palette_checksum_hex(palette),
        palette.transparent_index,
        palette.max_colors,
        palette.style_version,
        source
    );
    fs::write(output, normalized)
        .await
        .map_err(|e| format!("fallback normalize write: {e}"))?;
    Ok(())
}

async fn build_sprite_sheet_stub(
    input: &Path,
    output: &Path,
    spec: &AssetSpec,
    palette: &PaletteProfile,
) -> Result<(), String> {
    let content = fs::read_to_string(input)
        .await
        .map_err(|e| format!("read normalized: {e}"))?;
    let template = placeholder_template(&spec.asset_class);
    let preview = placeholder_preview(spec.width, spec.height, template.glyph);
    let sheet = format!(
        "SPRITE_SHEET_STUB_V1\nasset_class={:?}\nname={}\nframes={}\nframe_size={}x{}\npalette_id={}\npalette_checksum={}\nplaceholder_key={}\nplaceholder_preview:\n{}source:\n{}",
        spec.asset_class,
        spec.name,
        spec.frames,
        spec.width,
        spec.height,
        palette.id,
        palette_checksum_hex(palette),
        template.key,
        preview,
        content
    );
    fs::write(output, sheet)
        .await
        .map_err(|e| format!("write sprite sheet stub: {e}"))?;
    Ok(())
}

async fn build_metadata_stub(
    output: &Path,
    spec: &AssetSpec,
    lane: &GenerationLane,
    prompt_hash: &str,
    lane_decision: &LaneDecision,
    palette: &PaletteProfile,
    used_placeholder: bool,
) -> Result<(), String> {
    let template = placeholder_template(&spec.asset_class);
    let metadata = serde_json::json!({
        "name": spec.name,
        "asset_class": spec.asset_class,
        "lane": lane,
        "style_version": LOW_FI_STYLE_VERSION,
        "palette_id": palette.id,
        "palette_checksum": palette_checksum_hex(palette),
        "palette_colors": palette.colors,
        "palette_transparent_index": palette.transparent_index,
        "palette_max_colors": palette.max_colors,
        "dimensions": { "width": spec.width, "height": spec.height },
        "frames": spec.frames,
        "tags": spec.tags,
        "prompt_hash": prompt_hash,
        "routing_strategy": match lane_decision.strategy {
            RoutingStrategy::OpenAiOnly => "openai_only",
            RoutingStrategy::ClassRouter => "class_router",
        },
        "requested_lane": lane_decision.requested_lane.as_ref(),
        "lane_forced_by_strategy": lane_decision.forced_by_strategy,
        "placeholder_used": used_placeholder,
        "placeholder_key": template.key,
        "placeholder_summary": template.summary,
        "generated_at": Utc::now(),
    });
    fs::write(
        output,
        serde_json::to_vec_pretty(&metadata).map_err(|e| format!("metadata json: {e}"))?,
    )
    .await
    .map_err(|e| format!("write metadata: {e}"))?;
    Ok(())
}

async fn call_scenario_txt2img(
    spec: &AssetSpec,
    output_path: &Path,
    generated_image_path: &Path,
) -> Result<bool, String> {
    let api_key =
        std::env::var("SCENARIO_API_KEY").map_err(|_| "missing SCENARIO_API_KEY".to_string())?;
    let api_secret = std::env::var("SCENARIO_API_SECRET")
        .map_err(|_| "missing SCENARIO_API_SECRET".to_string())?;
    let url = std::env::var("SCENARIO_TXT2IMG_URL")
        .unwrap_or_else(|_| "https://api.cloud.scenario.com/v1/generate/txt2img".to_string());
    let model_id = std::env::var("SCENARIO_MODEL_ID").unwrap_or_else(|_| "pixel-art".to_string());

    let payload = serde_json::json!({
        "modelId": model_id,
        "prompt": spec.prompt,
        "numImages": 1,
        "width": spec.width,
        "height": spec.height
    });

    let client = Client::new();
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .header("x-api-key", api_key.clone())
        .header("x-api-secret", api_secret)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("scenario request failed: {e}"))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("scenario response read failed: {e}"))?;
    fs::write(output_path, &text)
        .await
        .map_err(|e| format!("write scenario raw output: {e}"))?;

    if !status.is_success() {
        return Err(format!("scenario http status {status} body={text}"));
    }

    let parsed: Value = serde_json::from_str(&text)
        .map_err(|e| format!("scenario response json parse failed: {e}"))?;
    if let Some(url) = extract_first_http_url(&parsed) {
        let img = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("download generated image failed: {e}"))?;
        if img.status().is_success() {
            let bytes = img
                .bytes()
                .await
                .map_err(|e| format!("read generated image bytes failed: {e}"))?;
            fs::write(generated_image_path, bytes)
                .await
                .map_err(|e| format!("write generated image: {e}"))?;
            return Ok(true);
        }
    }
    Ok(false)
}

async fn call_openai_images(
    spec: &AssetSpec,
    output_path: &Path,
    generated_image_path: &Path,
) -> Result<bool, String> {
    let api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| "missing OPENAI_API_KEY".to_string())?;
    let url = std::env::var("OPENAI_IMAGE_URL")
        .unwrap_or_else(|_| "https://api.openai.com/v1/images/generations".to_string());
    let model = std::env::var("OPENAI_IMAGE_MODEL").unwrap_or_else(|_| "gpt-image-1".to_string());
    let size = std::env::var("OPENAI_IMAGE_SIZE").unwrap_or_else(|_| "1024x1024".to_string());

    let payload = serde_json::json!({
        "model": model,
        "prompt": spec.prompt,
        "size": size,
        "n": 1
    });

    let client = Client::new();
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("openai request failed: {e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("openai response read failed: {e}"))?;
    fs::write(output_path, &text)
        .await
        .map_err(|e| format!("write openai raw output: {e}"))?;
    if !status.is_success() {
        return Err(format!("openai http status {status} body={text}"));
    }

    let parsed: Value =
        serde_json::from_str(&text).map_err(|e| format!("openai response parse failed: {e}"))?;
    if let Some(data) = parsed.get("data").and_then(|v| v.as_array()) {
        if let Some(first) = data.first() {
            if let Some(b64) = first.get("b64_json").and_then(|v| v.as_str()) {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(b64.as_bytes())
                    .map_err(|e| format!("openai image b64 decode failed: {e}"))?;
                fs::write(generated_image_path, bytes)
                    .await
                    .map_err(|e| format!("write openai generated image: {e}"))?;
                return Ok(true);
            }
            if let Some(url) = first.get("url").and_then(|v| v.as_str()) {
                return download_image(url, generated_image_path).await;
            }
        }
    }
    if let Some(url) = extract_first_http_url(&parsed) {
        return download_image(&url, generated_image_path).await;
    }
    Ok(false)
}

async fn call_replicate_prediction(
    spec: &AssetSpec,
    output_path: &Path,
    generated_image_path: &Path,
) -> Result<bool, String> {
    let token = std::env::var("REPLICATE_API_TOKEN")
        .map_err(|_| "missing REPLICATE_API_TOKEN".to_string())?;
    let url = std::env::var("REPLICATE_PREDICTION_URL")
        .unwrap_or_else(|_| "https://api.replicate.com/v1/predictions".to_string());
    let version =
        std::env::var("REPLICATE_VERSION").map_err(|_| "missing REPLICATE_VERSION".to_string())?;

    let payload = serde_json::json!({
        "version": version,
        "input": {
            "prompt": spec.prompt,
            "width": spec.width,
            "height": spec.height
        }
    });

    let client = Client::new();
    let response = client
        .post(url)
        .header("content-type", "application/json")
        .header("authorization", format!("Token {token}"))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("replicate request failed: {e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("replicate response read failed: {e}"))?;
    fs::write(output_path, &text)
        .await
        .map_err(|e| format!("write replicate raw output: {e}"))?;
    if !status.is_success() {
        return Err(format!("replicate http status {status} body={text}"));
    }

    let parsed: Value =
        serde_json::from_str(&text).map_err(|e| format!("replicate response parse failed: {e}"))?;
    if let Some(output) = parsed.get("output") {
        if let Some(url) = extract_first_http_url(output) {
            return download_image(&url, generated_image_path).await;
        }
    }
    if let Some(url) = extract_first_http_url(&parsed) {
        return download_image(&url, generated_image_path).await;
    }
    Ok(false)
}

async fn download_image(url: &str, output_path: &Path) -> Result<bool, String> {
    let client = Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download image failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("download image status {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("download image bytes read failed: {e}"))?;
    fs::write(output_path, bytes)
        .await
        .map_err(|e| format!("write downloaded image failed: {e}"))?;
    Ok(true)
}

async fn call_comfyui_workflow(
    spec: &AssetSpec,
    output_path: &Path,
    generated_image_path: &Path,
) -> Result<bool, String> {
    let base = std::env::var("COMFY_API_URL").map_err(|_| "missing COMFY_API_URL".to_string())?;
    let workflow_file = std::env::var("COMFY_WORKFLOW_FILE")
        .map_err(|_| "missing COMFY_WORKFLOW_FILE".to_string())?;
    let output_node = std::env::var("COMFY_OUTPUT_NODE_ID").ok();
    let client_id = Uuid::new_v4().to_string();

    let workflow_bytes = fs::read(&workflow_file)
        .await
        .map_err(|e| format!("read COMFY_WORKFLOW_FILE failed: {e}"))?;
    let mut prompt_json: Value = serde_json::from_slice(&workflow_bytes)
        .map_err(|e| format!("parse workflow json failed: {e}"))?;
    substitute_prompt_text(&mut prompt_json, &spec.prompt);

    let submit_payload = serde_json::json!({
        "prompt": prompt_json,
        "client_id": client_id
    });
    let submit_url = format!("{}/prompt", base.trim_end_matches('/'));
    let client = Client::new();
    let submit_resp = apply_comfy_auth(
        client
            .post(&submit_url)
            .header("content-type", "application/json")
            .json(&submit_payload),
    )
    .send()
    .await
    .map_err(|e| format!("comfy submit failed: {e}"))?;
    let submit_status = submit_resp.status();
    let submit_text = submit_resp
        .text()
        .await
        .map_err(|e| format!("comfy submit body read failed: {e}"))?;
    fs::write(output_path, &submit_text)
        .await
        .map_err(|e| format!("write comfy raw output failed: {e}"))?;
    if !submit_status.is_success() {
        return Err(format!(
            "comfy submit status {submit_status} body={submit_text}"
        ));
    }
    let submit_json: Value = serde_json::from_str(&submit_text)
        .map_err(|e| format!("parse comfy submit json failed: {e}"))?;
    let prompt_id = submit_json
        .get("prompt_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "comfy prompt_id missing".to_string())?
        .to_string();

    let history_url = format!("{}/history/{}", base.trim_end_matches('/'), prompt_id);
    let mut history_json = None::<Value>;
    for _ in 0..40 {
        let resp = apply_comfy_auth(client.get(&history_url))
            .send()
            .await
            .map_err(|e| format!("comfy history request failed: {e}"))?;
        if !resp.status().is_success() {
            sleep(Duration::from_millis(500)).await;
            continue;
        }
        let txt = resp
            .text()
            .await
            .map_err(|e| format!("comfy history body read failed: {e}"))?;
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            if find_comfy_image_descriptor(&v, output_node.as_deref()).is_some() {
                history_json = Some(v);
                break;
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
    let history = history_json
        .ok_or_else(|| "comfy history did not produce image output in time".to_string())?;
    if let Some((filename, subfolder, image_type)) =
        find_comfy_image_descriptor(&history, output_node.as_deref())
    {
        let view_url = format!(
            "{}/view?filename={}&subfolder={}&type={}",
            base.trim_end_matches('/'),
            urlencoding::encode(&filename),
            urlencoding::encode(&subfolder),
            urlencoding::encode(&image_type)
        );
        return download_image_comfy(&view_url, generated_image_path).await;
    }
    Ok(false)
}

fn apply_comfy_auth(builder: RequestBuilder) -> RequestBuilder {
    let header_name = std::env::var("COMFY_AUTH_HEADER").ok();
    let token = std::env::var("COMFY_AUTH_TOKEN").ok();
    if let (Some(h), Some(t)) = (header_name, token) {
        if !h.trim().is_empty() && !t.trim().is_empty() {
            return builder.header(h, t);
        }
    }
    let basic_user = std::env::var("COMFY_BASIC_USER").ok();
    let basic_pass = std::env::var("COMFY_BASIC_PASS").ok();
    if let (Some(u), Some(p)) = (basic_user, basic_pass) {
        if !u.trim().is_empty() {
            return builder.basic_auth(u, Some(p));
        }
    }
    builder
}

async fn download_image_comfy(url: &str, output_path: &Path) -> Result<bool, String> {
    let client = Client::new();
    let response = apply_comfy_auth(client.get(url))
        .send()
        .await
        .map_err(|e| format!("download comfy image failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("download comfy image status {}", response.status()));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("download comfy image bytes read failed: {e}"))?;
    fs::write(output_path, bytes)
        .await
        .map_err(|e| format!("write comfy image failed: {e}"))?;
    Ok(true)
}

fn substitute_prompt_text(prompt_json: &mut Value, prompt: &str) {
    if let Some(obj) = prompt_json.as_object_mut() {
        for node in obj.values_mut() {
            if let Some(inputs) = node.get_mut("inputs").and_then(|v| v.as_object_mut()) {
                if let Some(text) = inputs.get_mut("text") {
                    *text = Value::String(prompt.to_string());
                }
            }
        }
    }
}

fn find_comfy_image_descriptor(
    value: &Value,
    node_id_filter: Option<&str>,
) -> Option<(String, String, String)> {
    if let Some(root) = value.as_object() {
        for (_prompt_id, run_data) in root {
            if let Some(outputs) = run_data.get("outputs").and_then(|v| v.as_object()) {
                for (node_id, node_data) in outputs {
                    if node_id_filter.is_some_and(|filter| filter != node_id) {
                        continue;
                    }
                    if let Some(images) = node_data.get("images").and_then(|v| v.as_array()) {
                        for image in images {
                            let filename = image.get("filename").and_then(|v| v.as_str())?;
                            let subfolder = image
                                .get("subfolder")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let image_type = image
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("output");
                            return Some((
                                filename.to_string(),
                                subfolder.to_string(),
                                image_type.to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_first_http_url(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => {
            if s.starts_with("http://") || s.starts_with("https://") {
                Some(s.clone())
            } else {
                None
            }
        }
        Value::Array(items) => items.iter().find_map(extract_first_http_url),
        Value::Object(map) => {
            let prioritized = ["url", "imageUrl", "image_url", "uri", "src", "cdnUrl"];
            for key in prioritized {
                if let Some(v) = map.get(key) {
                    if let Some(found) = extract_first_http_url(v) {
                        return Some(found);
                    }
                }
            }
            map.values().find_map(extract_first_http_url)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_spec() -> AssetSpec {
        AssetSpec {
            tower_id: "tower-1".to_string(),
            floor_id: "floor-a".to_string(),
            department_id: "coding".to_string(),
            name: "ui_state_healthy".to_string(),
            asset_class: AssetClass::UiOverlay,
            width: 16,
            height: 16,
            frames: 2,
            tags: vec!["ui".to_string(), "state".to_string()],
            palette_id: "tower_v1".to_string(),
            prompt: "low-fi healthy overlay".to_string(),
            preferred_lane: None,
        }
    }

    #[test]
    fn lane_routing_is_stable() {
        assert_eq!(
            route_lane(&AssetClass::AgentSprite),
            GenerationLane::Scenario
        );
        assert_eq!(
            route_lane(&AssetClass::UiOverlay),
            GenerationLane::OpenAiImages
        );
        assert_eq!(
            route_lane(&AssetClass::DepartmentProp),
            GenerationLane::Replicate
        );
    }

    #[test]
    fn openai_only_strategy_forces_lane() {
        let mut spec = valid_spec();
        spec.preferred_lane = Some(GenerationLane::ComfyUi);
        let decision = resolve_lane_with_strategy(&spec, RoutingStrategy::OpenAiOnly);
        assert_eq!(decision.lane, GenerationLane::OpenAiImages);
        assert!(decision.forced_by_strategy);
    }

    #[test]
    fn class_router_strategy_keeps_requested_lane() {
        let mut spec = valid_spec();
        spec.preferred_lane = Some(GenerationLane::Replicate);
        let decision = resolve_lane_with_strategy(&spec, RoutingStrategy::ClassRouter);
        assert_eq!(decision.lane, GenerationLane::Replicate);
        assert!(!decision.forced_by_strategy);
    }

    #[test]
    fn spec_validation_blocks_bad_inputs() {
        let mut bad = valid_spec();
        bad.width = 0;
        assert!(validate_spec(&bad).is_err());
    }

    #[test]
    fn spec_validation_accepts_v1_profile() {
        assert!(validate_spec(&valid_spec()).is_ok());
    }

    #[test]
    fn spec_validation_rejects_unknown_palette() {
        let mut bad = valid_spec();
        bad.palette_id = "tower_v2".to_string();
        let err = validate_spec(&bad).expect_err("palette should be rejected");
        assert!(err.contains("unsupported palette_id"));
    }

    #[test]
    fn every_asset_class_has_placeholder_template() {
        for class in [
            AssetClass::AgentSprite,
            AssetClass::DepartmentProp,
            AssetClass::EnvironmentTile,
            AssetClass::UiOverlay,
            AssetClass::Fx,
            AssetClass::Portrait,
        ] {
            let template = placeholder_template(&class);
            assert!(!template.key.is_empty());
            assert!(template.glyph.is_ascii());
        }
    }

    #[test]
    fn routing_strategy_parse_defaults_to_openai_only() {
        assert_eq!(
            parse_routing_strategy("openai_only"),
            RoutingStrategy::OpenAiOnly
        );
        assert_eq!(
            parse_routing_strategy("class_router"),
            RoutingStrategy::ClassRouter
        );
        assert_eq!(
            parse_routing_strategy("unknown_mode"),
            RoutingStrategy::OpenAiOnly
        );
    }
}
