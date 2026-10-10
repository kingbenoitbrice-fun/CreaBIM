use glam::{Mat4, Vec2, Vec3};

/// Orbit camera around a target point, Z up.
#[derive(Clone, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(5.0, 3.0, 2.0),
            yaw: -2.2,
            pitch: 0.55,
            distance: 24.0,
            fov_y: 45f32.to_radians(),
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        let (sp, cp) = self.pitch.sin_cos();
        let (sy, cy) = self.yaw.sin_cos();
        self.target + Vec3::new(cp * cy, cp * sy, sp) * self.distance
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Z);
        let proj = Mat4::perspective_rh(self.fov_y, aspect.max(0.01), 0.05, 2000.0);
        proj * view
    }

    /// World-space ray through a point given in normalised device coordinates.
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let near = inv.project_point3(ndc.extend(0.0));
        let far = inv.project_point3(ndc.extend(1.0));
        (near, (far - near).normalize())
    }

    pub fn orbit(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.008;
        self.pitch = (self.pitch + delta.y * 0.008).clamp(-1.55, 1.55);
    }

    pub fn pan(&mut self, delta: Vec2, viewport_height: f32) {
        let eye = self.eye();
        let fwd = (self.target - eye).normalize();
        let right = fwd.cross(Vec3::Z).normalize_or_zero();
        let up = right.cross(fwd);
        let k = 2.0 * self.distance * (self.fov_y / 2.0).tan() / viewport_height.max(1.0);
        self.target += (-right * delta.x + up * delta.y) * k;
    }

    pub fn zoom(&mut self, steps: f32) {
        self.distance = (self.distance * (1.0 - steps * 0.1).clamp(0.5, 1.5)).clamp(0.5, 1500.0);
    }

    /// Fit a bounding box in view, for a viewport of the given aspect ratio.
    pub fn frame(&mut self, lo: Vec3, hi: Vec3, aspect: f32) {
        self.target = (lo + hi) / 2.0;
        let radius = (hi - lo).length().max(1.0) / 2.0;
        let half_fov_x = ((self.fov_y / 2.0).tan() * aspect.max(0.1)).atan();
        let half_fov = half_fov_x.min(self.fov_y / 2.0);
        self.distance = radius / half_fov.sin() * 1.05;
    }
}
