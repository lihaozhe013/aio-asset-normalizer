use three_d::*;

pub struct OrbitCamera {
    pub camera: Camera,
    target: Vec3,
    radius: f32,
    theta: f32,
    phi: f32,
    min_radius: f32,
    max_radius: f32,
    rotating: bool,
    panning: bool,
}

impl OrbitCamera {
    pub fn new(viewport: Viewport) -> Self {
        let position = vec3(4.0, 3.0, 6.0);
        let target = vec3(0.0, 0.5, 0.0);
        let direction = position - target;
        let radius = direction.magnitude();
        let theta = f32::atan2(direction.z, direction.x);
        let phi = f32::acos(direction.y / radius);

        let camera = Camera::new_perspective(
            viewport,
            position,
            target,
            vec3(0.0, 1.0, 0.0),
            degrees(45.0),
            0.1,
            100.0,
        );

        Self {
            camera,
            target,
            radius,
            theta,
            phi,
            min_radius: 0.5,
            max_radius: 50.0,
            rotating: false,
            panning: false,
        }
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        if viewport.width < 1 || viewport.height < 1 {
            return;
        }
        self.camera.set_viewport(viewport);
    }

    pub fn handle_events(&mut self, events: &[Event]) {
        for event in events {
            match event {
                Event::MousePress { button, .. } => match button {
                    MouseButton::Right => {
                        self.rotating = true;
                    }
                    MouseButton::Middle => {
                        self.panning = true;
                    }
                    _ => {}
                },
                Event::MouseRelease { button, .. } => match button {
                    MouseButton::Right => self.rotating = false,
                    MouseButton::Middle => self.panning = false,
                    _ => {}
                },
                Event::MouseMotion { delta, .. } => {
                    if self.rotating {
                        let sensitivity = 0.005;
                        self.theta -= delta.0 as f32 * sensitivity;
                        self.phi -= delta.1 as f32 * sensitivity;
                        self.phi = self.phi.clamp(0.05, std::f32::consts::PI - 0.05);
                        self.update_camera_view();
                    }
                    if self.panning {
                        let sensitivity = self.radius * 0.001;
                        let forward = (self.target - self.camera_position()).normalize();
                        let right = forward.cross(vec3(0.0, 1.0, 0.0));
                        let up = right.cross(forward);
                        self.target -= right * (delta.0 as f32 * sensitivity);
                        self.target += up * (delta.1 as f32 * sensitivity);
                        self.update_camera_view();
                    }
                }
                Event::MouseWheel { delta, .. } => {
                    self.radius -= delta.1 as f32 * 0.5;
                    self.radius = self.radius.clamp(self.min_radius, self.max_radius);
                    self.update_camera_view();
                }
                _ => {}
            }
        }
    }

    pub fn reset(&mut self) {
        let position = vec3(4.0, 3.0, 6.0);
        self.target = vec3(0.0, 0.5, 0.0);
        let direction = position - self.target;
        self.radius = direction.magnitude();
        self.theta = f32::atan2(direction.z, direction.x);
        self.phi = f32::acos(direction.y / self.radius);
        self.update_camera_view();
    }

    fn camera_position(&self) -> Vec3 {
        let x = self.radius * self.phi.sin() * self.theta.cos();
        let y = self.radius * self.phi.cos();
        let z = self.radius * self.phi.sin() * self.theta.sin();
        vec3(x, y, z) + self.target
    }

    fn update_camera_view(&mut self) {
        let position = self.camera_position();
        self.camera = Camera::new_perspective(
            self.camera.viewport(),
            position,
            self.target,
            vec3(0.0, 1.0, 0.0),
            degrees(45.0),
            0.1,
            100.0,
        );
    }
}
