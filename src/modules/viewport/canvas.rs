use super::helpers;
use crate::modules::preferences::ViewPreferences;
use std::path::Path;
use three_d::*;

pub struct ViewportCanvas {
    pub axes: Vec<Gm<Mesh, ColorMaterial>>,
    pub grid: Vec<Gm<Mesh, ColorMaterial>>,
    pub origin_sphere: Gm<Mesh, ColorMaterial>,
    pub skeleton: Vec<Gm<Mesh, ColorMaterial>>,
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
            skeleton: Vec::new(),
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

    pub fn load_glb(
        &mut self,
        context: &Context,
        path: &Path,
    ) -> Result<(), String> {
        let mut raw = three_d_asset::io::load(&[path])
            .map_err(|e| format!("Asset load error: {}", e))?;

        let mut cpu_model: CpuModel = raw
            .deserialize(path)
            .map_err(|e| format!("No model found in GLB: {}", e))?;

        for geom in cpu_model.geometries.iter_mut() {
            if let three_d_asset::Geometry::Triangles(ref mut mesh) =
                geom.geometry
            {
                if mesh.tangents.is_none()
                    && mesh.normals.is_some()
                    && mesh.uvs.is_some()
                {
                    mesh.compute_tangents();
                }
            }
        }

        let model =
            Model::new(context, &cpu_model).map_err(|e| e.to_string())?;

        self.model = Some(model);
        self.show_origin = false;
        Ok(())
    }

    pub fn model_lights(&self) -> [&dyn Light; 2] {
        [&self.ambient_light, &self.directional_light]
    }

    pub fn set_bvh_skeleton(
        &mut self,
        context: &Context,
        positions: &[[f32; 3]],
        parents: &[Option<usize>],
    ) {
        self.skeleton = helpers::build_skeleton(context, positions, parents);
    }

    pub fn clear_bvh_skeleton(&mut self) {
        self.skeleton.clear();
    }

    pub fn apply_view_prefs(&mut self, prefs: &ViewPreferences) {
        self.show_grid = prefs.show_grid;
        self.show_axes = prefs.show_axes;
        self.show_origin = prefs.show_origin;
    }

    pub fn to_view_prefs(&self) -> ViewPreferences {
        ViewPreferences {
            show_grid: self.show_grid,
            show_axes: self.show_axes,
            show_origin: self.show_origin,
        }
    }
}
