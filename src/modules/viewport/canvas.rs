use super::helpers;
use std::path::Path;
use three_d::*;

pub struct ViewportCanvas {
    pub axes: Gm<Mesh, ColorMaterial>,
    pub grid: Gm<Mesh, ColorMaterial>,
    pub origin_sphere: Gm<Mesh, ColorMaterial>,
    pub model: Option<Model<PhysicalMaterial>>,
    pub show_axes: bool,
    pub show_grid: bool,
    pub show_origin: bool,
    ambient_light: AmbientLight,
    directional_light: DirectionalLight,
}

impl ViewportCanvas {
    pub fn new(context: &Context) -> Self {
        Self {
            axes: helpers::build_axes(context),
            grid: helpers::build_grid(context),
            origin_sphere: helpers::build_origin_sphere(context),
            model: None,
            show_axes: true,
            show_grid: true,
            show_origin: true,
            ambient_light: AmbientLight {
                intensity: 0.3,
                color: Srgba::new(255, 255, 255, 255),
                environment: None,
            },
            directional_light: DirectionalLight::new(
                context,
                0.8,
                Srgba::new(255, 255, 255, 255),
                vec3(-1.0, -2.0, -1.0),
            ),
        }
    }

    pub fn load_glb(&mut self, context: &Context, path: &Path) -> Result<(), String> {
        let mut raw =
            three_d_asset::io::load(&[path]).map_err(|e| format!("Asset load error: {}", e))?;

        let cpu_model: CpuModel = raw
            .deserialize("Scene")
            .or_else(|_| raw.deserialize("scene"))
            .map_err(|e| format!("No model found in GLB: {}", e))?;

        let model = Model::new(context, &cpu_model).map_err(|e| e.to_string())?;

        self.model = Some(model);
        Ok(())
    }

    pub fn model_lights(&self) -> [&dyn Light; 2] {
        [&self.ambient_light, &self.directional_light]
    }
}
