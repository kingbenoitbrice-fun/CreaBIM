use glam::{Mat4, Quat, Vec2, Vec3};

/// Orbit camera around a target point, Z up.
#[derive(Clone, Debug)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
    /// Orthographic (parallel) projection instead of perspective. The
    /// visible height is the one perspective has at the target.
    pub ortho: bool,
}

/// The standard directions to look from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Top,
    Front,
    Right,
    Back,
    Left,
    /// The default three-quarter view from the south-west.
    Iso,
}

impl Look {
    pub const ALL: [Look; 6] = [
        Look::Top,
        Look::Front,
        Look::Right,
        Look::Back,
        Look::Left,
        Look::Iso,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Look::Top => "Top",
            Look::Front => "Front",
            Look::Right => "Right",
            Look::Back => "Back",
            Look::Left => "Left",
            Look::Iso => "Iso",
        }
    }

    /// Yaw and pitch of the eye round the target. Front looks north, from
    /// the south.
    fn angles(self) -> (f32, f32) {
        use std::f32::consts::FRAC_PI_2;
        match self {
            Look::Top => (-FRAC_PI_2, 1.55),
            Look::Front => (-FRAC_PI_2, 0.0),
            Look::Right => (0.0, 0.0),
            Look::Back => (FRAC_PI_2, 0.0),
            Look::Left => (std::f32::consts::PI, 0.0),
            Look::Iso => (-2.2, 0.55),
        }
    }
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::new(5.0, 3.0, 2.0),
            yaw: -2.2,
            pitch: 0.55,
            distance: 24.0,
            fov_y: 45f32.to_radians(),
            ortho: false,
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
        let aspect = aspect.max(0.01);
        let proj = if self.ortho {
            let h = self.half_height();
            // the near plane sits well behind the eye so nothing is clipped
            Mat4::orthographic_rh(-h * aspect, h * aspect, -h, h, -2000.0, 4000.0)
        } else {
            Mat4::perspective_rh(self.fov_y, aspect, 0.05, 2000.0)
        };
        proj * view
    }

    /// World-space ray through a point given in normalised device coordinates.
    pub fn ray(&self, ndc: Vec2, aspect: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(aspect).inverse();
        let near = inv.project_point3(ndc.extend(0.0));
        let far = inv.project_point3(ndc.extend(1.0));
        (near, (far - near).normalize())
    }

    /// Half the height seen at the target's distance.
    fn half_height(&self) -> f32 {
        self.distance * (self.fov_y / 2.0).tan()
    }

    /// Look from a standard direction, keeping the target and distance.
    pub fn look(&mut self, look: Look) {
        (self.yaw, self.pitch) = look.angles();
    }

    /// Orbit about `pivot` instead of the target, as Blender's "orbit
    /// around selection": the whole camera swings round the pivot, so the
    /// pivot stays where it is on screen.
    pub fn orbit_around(&mut self, pivot: Vec3, delta: Vec2) {
        let dyaw = -delta.x * 0.008;
        let pitch = (self.pitch + delta.y * 0.008).clamp(-1.55, 1.55);
        let dir = (self.eye() - self.target).normalize_or_zero();
        let axis = dir.cross(Vec3::Z).normalize_or_zero();
        let turn = Quat::from_rotation_z(dyaw) * Quat::from_axis_angle(axis, pitch - self.pitch);
        self.target = pivot + turn * (self.target - pivot);
        self.yaw += dyaw;
        self.pitch = pitch;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbiting_round_a_pivot_keeps_it_on_screen() {
        let mut c = OrbitCamera::default();
        let pivot = Vec3::new(8.0, 1.0, 0.5);
        let at = |c: &OrbitCamera| c.view_proj(1.5).project_point3(pivot);
        let before = at(&c);
        c.orbit_around(pivot, Vec2::new(40.0, -25.0));
        let after = at(&c);
        assert!(
            (before - after).truncate().length() < 1e-3,
            "{before} {after}"
        );
        assert!((c.pitch - (0.55 - 0.2)).abs() < 1e-5);
    }

    #[test]
    fn orthographic_rays_are_parallel() {
        let c = OrbitCamera {
            ortho: true,
            ..Default::default()
        };
        let (_, a) = c.ray(Vec2::new(-0.8, 0.3), 1.5);
        let (_, b) = c.ray(Vec2::new(0.7, -0.6), 1.5);
        assert!(a.dot(b) > 0.9999);
    }
}
