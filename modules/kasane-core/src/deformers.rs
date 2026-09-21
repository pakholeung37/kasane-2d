use std::f32::consts::PI;

pub const TWO_PI: f32 = 2.0 * PI;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PsmVec2 {
    pub x: f32,
    pub y: f32,
}

impl std::ops::Add for PsmVec2 {
    type Output = Self;
    #[inline(always)]
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl std::ops::Sub for PsmVec2 {
    type Output = Self;
    #[inline(always)]
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl std::ops::Neg for PsmVec2 {
    type Output = Self;
    #[inline(always)]
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
}

impl PsmVec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn add(self, other: Self) -> Self {
        self + other
    }

    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn sub(self, other: Self) -> Self {
        self - other
    }

    #[allow(clippy::should_implement_trait)]
    #[inline(always)]
    pub fn neg(self) -> Self {
        -self
    }

    #[inline(always)]
    pub fn scale(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s)
    }

    #[inline(always)]
    // Use explicit fused operations to keep Core arithmetic stable across builds.
    pub fn bary3(a: Self, b: Self, c: Self, wa: f32, wb: f32, wc: f32) -> Self {
        Self::new(
            wc.mul_add(c.x, wb.mul_add(b.x, wa * a.x)),
            wc.mul_add(c.y, wb.mul_add(b.y, wa * a.y)),
        )
    }

    #[inline(always)]
    pub fn bilinear(p00: Self, p10: Self, p01: Self, p11: Self, u: f32, v: f32) -> Self {
        let inv_u = 1.0 - u;
        let x0 = u.mul_add(p10.x, inv_u * p00.x);
        let y0 = u.mul_add(p10.y, inv_u * p00.y);
        let x1 = u.mul_add(p11.x, inv_u * p01.x);
        let y1 = u.mul_add(p11.y, inv_u * p01.y);
        let inv_v = 1.0 - v;
        Self::new(v.mul_add(x1, inv_v * x0), v.mul_add(y1, inv_v * y0))
    }
}

#[inline(always)]
pub fn v2_load(arr: &[f32], idx: usize) -> PsmVec2 {
    PsmVec2::new(arr[idx * 2], arr[idx * 2 + 1])
}

#[inline(always)]
pub fn v2_store(arr: &mut [f32], idx: usize, v: PsmVec2) {
    arr[idx * 2] = v.x;
    arr[idx * 2 + 1] = v.y;
}

#[derive(Debug, Clone, Copy)]
pub struct WarpBasis {
    pub center: PsmVec2,
    pub dpdv: PsmVec2,
    pub dpdu: PsmVec2,
}

#[derive(Debug, Clone, Copy)]
pub struct WarpCell {
    pub fu: f32,
    pub fv: f32,
    pub p00: PsmVec2,
    pub p10: PsmVec2,
    pub p01: PsmVec2,
    pub p11: PsmVec2,
}

pub fn warp_extrap_basis(pos: &[f32], row: i32, col: i32, stride: i32) -> WarpBasis {
    let c00 = PsmVec2::new(pos[0], pos[1]);
    let c10 = v2_load(pos, col as usize);
    let c01 = v2_load(pos, (row * stride) as usize);
    let c11 = v2_load(pos, (row * stride + col) as usize);

    let d11_00 = c11.sub(c00);
    let d10_01 = c10.sub(c01);

    let dpdv = d11_00.sub(d10_01).scale(0.5);
    let dpdu = d10_01.add(d11_00).scale(0.5);

    let sum = c00.add(c10).add(c01.add(c11));
    let center = sum.scale(0.25).sub(d11_00.scale(0.5));

    WarpBasis { center, dpdv, dpdu }
}

#[allow(clippy::too_many_arguments)]
pub fn warp_extrap_cell(
    u: f32,
    v: f32,
    gu: f32,
    gv: f32,
    row: i32,
    col: i32,
    stride: i32,
    pos: &[f32],
    basis: &WarpBasis,
) -> WarpCell {
    let fr = row as f32;
    let fc = col as f32;
    let cen = basis.center;
    let dv = basis.dpdv;
    let du = basis.dpdu;

    let mut cell = WarpCell {
        fu: 0.0,
        fv: 0.0,
        p00: PsmVec2::default(),
        p10: PsmVec2::default(),
        p01: PsmVec2::default(),
        p11: PsmVec2::default(),
    };

    let mut cu = 0i32;
    let mut cv = 0i32;
    let mut uc = 0.0f32;
    let mut un = 0.0f32;
    let mut vc = 0.0f32;
    let mut vn = 0.0f32;

    if u <= 0.0 {
        cell.fu = (u + 2.0) * 0.5;
    } else if u >= 1.0 {
        cell.fu = (u - 1.0) * 0.5;
    } else {
        cu = gu as i32;
        if cu == col {
            cu = col - 1;
        }
        cell.fu = gu - cu as f32;
        uc = (cu as f32) / fc;
        un = ((cu + 1) as f32) / fc;
    }

    if v <= 0.0 {
        cell.fv = (v + 2.0) * 0.5;
    } else if v >= 1.0 {
        cell.fv = (v - 1.0) * 0.5;
    } else {
        cv = gv as i32;
        if cv == row {
            cv = row - 1;
        }
        cell.fv = gv - cv as f32;
        vc = (cv as f32) / fr;
        vn = ((cv + 1) as f32) / fr;
    }

    if u <= 0.0 {
        if v <= 0.0 {
            cell.p00 = cen.sub(dv.scale(2.0).add(du.scale(2.0)));
            cell.p10 = cen.sub(dv.scale(2.0));
            cell.p01 = cen.sub(du.scale(2.0));
            cell.p11 = PsmVec2::new(pos[0], pos[1]);
        } else if v < 1.0 {
            cell.p00 = cen.sub(du.scale(2.0)).add(dv.scale(vc));
            cell.p10 = v2_load(pos, (cv * stride) as usize);
            cell.p01 = cen.sub(du.scale(2.0)).add(dv.scale(vn));
            cell.p11 = v2_load(pos, ((cv + 1) * stride) as usize);
        } else {
            cell.p00 = cen.sub(du.scale(2.0)).add(dv);
            cell.p10 = v2_load(pos, (row * stride) as usize);
            cell.p01 = cen.sub(du.scale(2.0)).add(dv.scale(3.0));
            cell.p11 = cen.add(dv.scale(3.0));
        }
    } else if u < 1.0 {
        if v <= 0.0 {
            cell.p00 = du.scale(uc).add(cen.sub(dv.scale(2.0)));
            cell.p10 = du.scale(un).add(cen.sub(dv.scale(2.0)));
            cell.p01 = v2_load(pos, cu as usize);
            cell.p11 = v2_load(pos, (cu + 1) as usize);
        } else {
            cell.p00 = v2_load(pos, (row * stride + cu) as usize);
            cell.p10 = v2_load(pos, (row * stride + cu + 1) as usize);
            cell.p01 = cen.add(du.scale(uc)).add(dv.scale(3.0));
            cell.p11 = cen.add(du.scale(un)).add(dv.scale(3.0));
        }
    } else {
        if v <= 0.0 {
            cell.p00 = cen.sub(dv.scale(2.0)).add(du);
            cell.p10 = cen.sub(dv.scale(2.0)).add(du.scale(3.0));
            cell.p01 = v2_load(pos, col as usize);
            cell.p11 = cen.add(du.scale(3.0));
        } else if v < 1.0 {
            cell.p00 = v2_load(pos, (col + cv * stride) as usize);
            cell.p10 = cen.add(du.scale(3.0)).add(dv.scale(vc));
            cell.p01 = v2_load(pos, (col + (cv + 1) * stride) as usize);
            cell.p11 = cen.add(du.scale(3.0)).add(dv.scale(vn));
        } else {
            cell.p00 = v2_load(pos, (row * stride + col) as usize);
            cell.p10 = cen.add(du.scale(3.0)).add(dv);
            cell.p01 = cen.add(du.scale(3.0)).add(du);
            cell.p11 = cen.add(du.scale(3.0).add(dv.scale(3.0)));
        }
    }

    cell
}

#[inline(always)]
pub fn interp_triangle(cell: &WarpCell) -> PsmVec2 {
    let fu = cell.fu;
    let fv = cell.fv;
    if fu + fv <= 1.0 {
        let w00 = 1.0 - fu - fv;
        PsmVec2::bary3(cell.p00, cell.p10, cell.p01, w00, fu, fv)
    } else {
        let w10 = 1.0 - fv;
        let w11 = fu + fv - 1.0;
        let w01 = 1.0 - fu;
        PsmVec2::bary3(cell.p10, cell.p11, cell.p01, w10, w11, w01)
    }
}

pub fn warp_points(
    row: i32,
    col: i32,
    is_quad: bool,
    pos: &[f32],
    inputs: &[f32],
    outputs: &mut [f32],
    count: usize,
) {
    let stride = col + 1;
    let fr = row as f32;
    let fc = col as f32;

    let mut extrap_setup = false;
    let mut basis = WarpBasis {
        center: PsmVec2::default(),
        dpdv: PsmVec2::default(),
        dpdu: PsmVec2::default(),
    };

    for i in 0..count {
        let uv = v2_load(inputs, i);
        let gu = uv.x * fc;
        let gv = uv.y * fr;

        if uv.x >= 0.0 && uv.x < 1.0 && uv.y >= 0.0 && uv.y < 1.0 {
            let cu = gu as i32;
            let cv = gv as i32;
            let fu = gu - cu as f32;
            let fv = gv - cv as f32;

            let bi = (cv * stride + cu) as usize;
            let p00 = v2_load(pos, bi);
            let p10 = v2_load(pos, bi + 1);
            let p01 = v2_load(pos, bi + stride as usize);
            let p11 = v2_load(pos, bi + stride as usize + 1);

            let result = if is_quad {
                PsmVec2::bilinear(p00, p10, p01, p11, fu, fv)
            } else {
                let cell = WarpCell {
                    fu,
                    fv,
                    p00,
                    p10,
                    p01,
                    p11,
                };
                interp_triangle(&cell)
            };
            v2_store(outputs, i, result);
        } else {
            if !extrap_setup {
                basis = warp_extrap_basis(pos, row, col, stride);
                extrap_setup = true;
            }

            if uv.x > -2.0 && uv.x < 3.0 && uv.y > -2.0 && uv.y < 3.0 {
                let cell = warp_extrap_cell(uv.x, uv.y, gu, gv, row, col, stride, pos, &basis);
                let r = interp_triangle(&cell);
                v2_store(outputs, i, r);
            } else {
                let rx = basis.dpdu.x * uv.x + basis.center.x + basis.dpdv.x * uv.y;
                let ry = basis.dpdu.y * uv.x + basis.center.y + basis.dpdv.y * uv.y;
                outputs[i * 2] = rx;
                outputs[i * 2 + 1] = ry;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn rotation_points(
    base_angle: f32,
    angle: f32,
    scale: f32,
    origin: PsmVec2,
    rx: bool,
    ry: bool,
    inputs: &[f32],
    outputs: &mut [f32],
    count: usize,
) {
    let angle_rad = (base_angle + angle) * PI / 180.0;
    let (sin_a, cos_a) = angle_rad.sin_cos();

    let rxf = if rx { -1.0 } else { 1.0 };
    let ryf = if ry { -1.0 } else { 1.0 };

    let m00 = scale * cos_a * rxf;
    let m01 = scale * (-sin_a) * ryf;
    let m10 = scale * sin_a * rxf;
    let m11 = scale * cos_a * ryf;

    for i in 0..count {
        let p = v2_load(inputs, i);
        // Accumulate the linear part before translation. Interleaving origin
        // loses low bits that nested rotation-parent angle estimation amplifies.
        let r = PsmVec2::new(
            m00.mul_add(p.x, m01 * p.y) + origin.x,
            m10.mul_add(p.x, m11 * p.y) + origin.y,
        );
        v2_store(outputs, i, r);
    }
}

pub fn plain_signed_angle(a: &[f32; 2], b: &[f32; 2]) -> f32 {
    let diff = a[1].atan2(a[0]) - b[1].atan2(b[0]);
    let rem = diff % TWO_PI;
    if rem > PI {
        rem - TWO_PI
    } else if rem < -PI {
        rem + TWO_PI
    } else {
        rem
    }
}

pub fn rotation_parent_angle<F>(
    parent_rotation: bool,
    mut transform: F,
    inout_origin: &mut PsmVec2,
) -> f32
where
    F: FnMut(PsmVec2) -> PsmVec2,
{
    let origin = *inout_origin;
    let mut direction = PsmVec2::new(0.0, 0.0);
    let dir_delta = if parent_rotation { -10.0 } else { -0.1 };

    let t_origin = transform(origin);
    let mut scale = 1.0f32;

    for _ in 0..16 {
        let tp = PsmVec2::new(origin.x, origin.y + scale * dir_delta);
        let tt = transform(tp);
        let d = tt.sub(t_origin);

        if d.x != 0.0 || d.y != 0.0 {
            direction = d;
            break;
        }

        let tp2 = PsmVec2::new(origin.x, origin.y - scale * dir_delta);
        let tt2 = transform(tp2);
        let d2 = tt2.sub(t_origin);

        if d2.x != 0.0 || d2.y != 0.0 {
            direction = d2.neg();
            break;
        }

        scale *= 0.1;
    }

    let base_dir = [0.0, dir_delta];
    let dir_arr = [direction.x, direction.y];
    let angle_adj = (plain_signed_angle(&base_dir, &dir_arr) * -180.0) / PI;

    *inout_origin = transform(origin);
    angle_adj
}
