//! Scene model: resolution-independent drawing primitives and validation.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Clone, Debug)]
pub enum Fill {
    Solid(Rgba),
}

#[derive(Clone, Debug)]
pub enum Geom {
    Polyline {
        points: Vec<(f32, f32)>,
        closed: bool,
    },
    Arc {
        cx: f32,
        cy: f32,
        r: f32,
        a0: f32,
        a1: f32,
    },
    Disc {
        cx: f32,
        cy: f32,
        r: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    /// Filled radial gradient: fill color at center → transparent at r.
    RadialGlow {
        cx: f32,
        cy: f32,
        r: f32,
    },
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub geom: Geom,
    pub fill: Fill,
    /// Stroke width; 0.0 = filled (Disc/Rect/RadialGlow always filled).
    pub width: f32,
    /// 0..=1: renderers fake bloom (wide translucent under-stroke ×3 width).
    pub glow: f32,
    pub dash: Option<(f32, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub shapes: Vec<Shape>,
}

impl Scene {
    /// All coordinates finite and within ±4×max(w,h); widths ≥ 0; glow 0..=1.
    pub fn is_finite_and_sane(&self, width: f32, height: f32) -> bool {
        if !width.is_finite() || !height.is_finite() {
            return false;
        }
        let bound = 4.0 * width.max(height);

        for shape in &self.shapes {
            // Check width and glow
            if !shape.width.is_finite() || shape.width < 0.0 {
                return false;
            }
            if !shape.glow.is_finite() || !(0.0..=1.0).contains(&shape.glow) {
                return false;
            }

            // Check fill colors
            match &shape.fill {
                Fill::Solid(rgba) => {
                    if !rgba.r.is_finite()
                        || !rgba.g.is_finite()
                        || !rgba.b.is_finite()
                        || !rgba.a.is_finite()
                    {
                        return false;
                    }
                }
            }

            // Check geometry
            match &shape.geom {
                Geom::Polyline { points, closed: _ } => {
                    for (x, y) in points {
                        if !x.is_finite() || !y.is_finite() {
                            return false;
                        }
                        if x.abs() > bound || y.abs() > bound {
                            return false;
                        }
                    }
                }
                Geom::Arc { cx, cy, r, a0, a1 } => {
                    if !cx.is_finite()
                        || !cy.is_finite()
                        || !r.is_finite()
                        || !a0.is_finite()
                        || !a1.is_finite()
                    {
                        return false;
                    }
                    if cx.abs() > bound || cy.abs() > bound || r.abs() > bound {
                        return false;
                    }
                }
                Geom::Disc { cx, cy, r } => {
                    if !cx.is_finite() || !cy.is_finite() || !r.is_finite() {
                        return false;
                    }
                    if cx.abs() > bound || cy.abs() > bound || r.abs() > bound {
                        return false;
                    }
                }
                Geom::Rect { x, y, w, h } => {
                    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
                        return false;
                    }
                    if x.abs() > bound || y.abs() > bound || w.abs() > bound || h.abs() > bound {
                        return false;
                    }
                }
                Geom::RadialGlow { cx, cy, r } => {
                    if !cx.is_finite() || !cy.is_finite() || !r.is_finite() {
                        return false;
                    }
                    if cx.abs() > bound || cy.abs() > bound || r.abs() > bound {
                        return false;
                    }
                }
            }
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanity_accepts_bounded_and_rejects_nan() {
        let ok = Shape {
            geom: Geom::Disc {
                cx: 10.0,
                cy: 10.0,
                r: 3.0,
            },
            fill: Fill::Solid(Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.5,
            }),
            width: 0.0,
            glow: 0.0,
            dash: None,
        };
        assert!(Scene {
            shapes: vec![ok.clone()]
        }
        .is_finite_and_sane(100.0, 100.0));
        let mut bad = ok;
        bad.geom = Geom::Disc {
            cx: f32::NAN,
            cy: 10.0,
            r: 3.0,
        };
        assert!(!Scene { shapes: vec![bad] }.is_finite_and_sane(100.0, 100.0));
    }
}
