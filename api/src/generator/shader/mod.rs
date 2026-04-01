pub mod water_shader;

use crate::generator::shader::water_shader::WaterGfxPlugin;
use bevy::prelude::*;

pub struct WorldShaderService;
impl Plugin for WorldShaderService {
    fn build(&self, app: &mut App) {
        app.add_plugins(WaterGfxPlugin);
    }
}
