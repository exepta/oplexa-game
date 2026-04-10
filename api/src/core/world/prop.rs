use serde::Deserialize;

const PROP_MIN_SIZE_METERS: f32 = 0.05;
const PROP_MAX_WIDTH_METERS: f32 = 1.0;
const PROP_MAX_HEIGHT_METERS: f32 = 2.5;
const PROP_DEFAULT_WIDTH_METERS: f32 = 1.0;
const PROP_DEFAULT_HEIGHT_METERS: f32 = 1.0;
const PROP_DEFAULT_PLANE_COUNT: u8 = 2;
const PROP_MAX_TILT_DEG: f32 = 35.0;

/// Defines supported prop render variants for world blocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PropRenderKind {
    #[default]
    CrossedPlanes,
}

/// Runtime prop definition attached to one block entry.
#[derive(Clone, Debug, Deserialize)]
pub struct PropDefinition {
    #[serde(default)]
    pub render: PropRenderKind,
    #[serde(default = "default_prop_width_m")]
    pub width_m: f32,
    #[serde(default = "default_prop_height_m")]
    pub height_m: f32,
    #[serde(default = "default_prop_plane_count")]
    pub plane_count: u8,
    /// Optional visual lean angle in degrees for crossed-plane props.
    #[serde(default)]
    pub tilt_deg: f32,
    /// Optional allow-list of ground block names this prop can stand on.
    #[serde(
        default,
        alias = "allowed_on",
        alias = "allowed_below",
        alias = "allowed_support"
    )]
    pub allowed_ground: Vec<String>,
}

impl PropDefinition {
    /// Returns a sanitized copy with safe limits for runtime meshing.
    pub fn sanitized(mut self) -> Self {
        self.width_m = self
            .width_m
            .clamp(PROP_MIN_SIZE_METERS, PROP_MAX_WIDTH_METERS);
        self.height_m = self
            .height_m
            .clamp(PROP_MIN_SIZE_METERS, PROP_MAX_HEIGHT_METERS);
        self.plane_count = self.plane_count.clamp(2, 6);
        self.tilt_deg = self.tilt_deg.clamp(0.0, PROP_MAX_TILT_DEG);
        let mut cleaned: Vec<String> = Vec::new();
        for raw in self.allowed_ground {
            let name = raw.trim();
            if name.is_empty() {
                continue;
            }
            if cleaned.iter().any(|entry| entry.eq_ignore_ascii_case(name)) {
                continue;
            }
            cleaned.push(name.to_string());
        }
        self.allowed_ground = cleaned;
        self
    }

    /// Returns true when this prop should render as crossed planes.
    #[inline]
    pub fn is_crossed_planes(&self) -> bool {
        matches!(self.render, PropRenderKind::CrossedPlanes)
    }

    /// Returns true if this prop allows placement on the given ground block name.
    #[inline]
    pub fn allows_ground_name(&self, ground_name: &str) -> bool {
        if self.allowed_ground.is_empty() {
            return true;
        }
        self.allowed_ground
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(ground_name))
    }
}

impl Default for PropDefinition {
    fn default() -> Self {
        Self {
            render: PropRenderKind::CrossedPlanes,
            width_m: PROP_DEFAULT_WIDTH_METERS,
            height_m: PROP_DEFAULT_HEIGHT_METERS,
            plane_count: PROP_DEFAULT_PLANE_COUNT,
            tilt_deg: 0.0,
            allowed_ground: Vec::new(),
        }
    }
}

#[inline]
fn default_prop_width_m() -> f32 {
    PROP_DEFAULT_WIDTH_METERS
}

#[inline]
fn default_prop_height_m() -> f32 {
    PROP_DEFAULT_HEIGHT_METERS
}

#[inline]
fn default_prop_plane_count() -> u8 {
    PROP_DEFAULT_PLANE_COUNT
}
