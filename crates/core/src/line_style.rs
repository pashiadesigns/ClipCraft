//! Editable stroke geometry shared by rendering, hit tests and manipulation.
//! Endpoint local geometry/tangents/path trimming adapted from DesignCraft
//! crates/doc/src/arrow.rs, MIT, ea08e4c2ba290f092063c99d70c4bdd5afc45887.
//! Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors.
//! ClipDesigner adds independent endpoint sizes, circular/quadratic bend geometry,
//! bounded patterns, legacy preservation and its own annotation/command model.
use crate::{Annotation, Error, Kind, Result};
use serde::{Deserialize, Serialize};
use vello_cpu::kurbo::ParamCurveNearest;
use vello_cpu::kurbo::{
    Affine, Arc, BezPath, Circle, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathEl, PathSeg,
    Point, Shape, Vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endpoint {
    Flat,
    Round,
    OpenArrow,
    Triangle,
    WideArrow,
    Stealth,
    Circle,
    Square,
    Diamond,
    Bar,
}
impl Endpoint {
    pub const ALL: [Self; 10] = [
        Self::Flat,
        Self::Round,
        Self::OpenArrow,
        Self::Triangle,
        Self::WideArrow,
        Self::Stealth,
        Self::Circle,
        Self::Square,
        Self::Diamond,
        Self::Bar,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "Flat",
            Self::Round => "Round",
            Self::OpenArrow => "Open arrow",
            Self::Triangle => "Triangle",
            Self::WideArrow => "Wide arrow",
            Self::Stealth => "Notched arrow",
            Self::Circle => "Circle",
            Self::Square => "Square",
            Self::Diamond => "Diamond",
            Self::Bar => "Bar",
        }
    }
    pub fn sized(self) -> bool {
        !matches!(self, Self::Flat | Self::Round)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pattern {
    Solid,
    Dashed,
    Dotted,
    DashDot,
    LongDash,
    DashDotDot,
    Wavy,
}
/// Conservative outline reach shared by viewport, shadows and canvas expansion.
pub fn stroke_reach(a: &Annotation) -> f32 {
    a.width as f32
        * if a.line_style.is_some_and(|s| s.pattern == Pattern::Wavy) {
            1.0
        } else {
            0.5
        }
}
impl Pattern {
    pub const ALL: [Self; 7] = [
        Self::Solid,
        Self::Dashed,
        Self::Dotted,
        Self::DashDot,
        Self::LongDash,
        Self::DashDotDot,
        Self::Wavy,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Dashed => "Dashed",
            Self::Dotted => "Dotted",
            Self::DashDot => "Dash-dot",
            Self::LongDash => "Long dash",
            Self::DashDotDot => "Dash-dot-dot",
            Self::Wavy => "Wavy",
        }
    }
    pub fn dashes(self, width: f64) -> Vec<f64> {
        let units: &[f64] = match self {
            Self::Solid | Self::Wavy => &[],
            Self::Dashed => &[4., 3.],
            Self::Dotted => &[1., 2.],
            Self::DashDot => &[5., 2., 1., 2.],
            Self::LongDash => &[8., 3.],
            Self::DashDotDot => &[5., 2., 1., 2., 1., 2.],
        };
        units.iter().map(|n| n * width.max(1.)).collect()
    }

    /// Bounded, deterministic vector waves. Preserve corners, closure and tips;
    /// the fill and line endpoints retain their original geometry.
    pub fn path(self, path: &BezPath, width: f64) -> BezPath {
        if self != Self::Wavy {
            return path.clone();
        }
        let mut out = BezPath::new();
        let mut last = None;
        for seg in path.segments() {
            let start = seg.start();
            if last != Some(start) {
                out.move_to(start);
            }
            let length = seg.arclen(0.1);
            let cycles = (length / (width * 8.).max(12.)).round().clamp(1., 128.);
            let samples = (cycles as usize * 12).clamp(12, 1536);
            let amplitude = (width * 0.45).min(length * 0.06);
            for i in 1..=samples {
                let t = i as f64 / samples as f64;
                let point = seg.eval(t);
                let tangent = seg.eval((t + 0.0001).min(1.)) - seg.eval((t - 0.0001).max(0.));
                let offset = if i == samples || tangent.hypot() < 1e-9 {
                    Vec2::ZERO
                } else {
                    Vec2::new(-tangent.y, tangent.x) / tangent.hypot()
                        * (std::f64::consts::TAU * cycles * t).sin()
                        * amplitude
                };
                out.line_to(point + offset);
            }
            last = Some(seg.end());
        }
        if matches!(path.elements().last(), Some(PathEl::ClosePath)) {
            out.close_path();
        }
        out
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    Straight,
    Circular,
    Bezier,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Transient playback sampling only; never stored in editable projects.
    #[serde(skip)]
    pub reveal: Option<f64>,
    #[serde(skip)]
    pub reveal_from_end: bool,
    pub start: Endpoint,
    pub end: Endpoint,
    pub start_size: f32,
    pub end_size: f32,
    pub pattern: Pattern,
    pub curve: Curve,
    /// Midpoint sagitta as a fraction of chord length; invariant under scaling.
    pub bend: f32,
}
impl Style {
    pub fn validate_for(self, kind: Kind) -> Result<()> {
        self.validate()?;
        if matches!(
            kind,
            Kind::Line | Kind::Arrow | Kind::Pen | Kind::Highlighter
        ) {
            return Ok(());
        }
        if matches!(
            kind,
            Kind::Rectangle | Kind::Ellipse | Kind::Polygon | Kind::Callout | Kind::Step
        ) && (Self {
            pattern: Pattern::Solid,
            ..self
        }) == Self::for_kind(kind)
        {
            return Ok(());
        }
        Err(Error::Invalid(
            "Line settings require Lines; shape outlines support patterns only".into(),
        ))
    }
    pub fn for_kind(kind: Kind) -> Self {
        Self {
            reveal: None,
            reveal_from_end: false,
            start: Endpoint::Round,
            end: if kind == Kind::Arrow {
                Endpoint::OpenArrow
            } else {
                Endpoint::Round
            },
            start_size: 3.,
            end_size: 3.,
            pattern: Pattern::Solid,
            curve: Curve::Straight,
            bend: 0.,
        }
    }
    pub fn validate(self) -> Result<()> {
        if [self.start_size, self.end_size]
            .iter()
            .any(|n| !n.is_finite() || !(0.5..=12.).contains(n))
            || !self.bend.is_finite()
            || self.bend.abs() > 2.
            || (self.curve == Curve::Circular && self.bend.abs() > 0.5)
        {
            return Err(Error::Invalid("Endpoint sizes must be 0.5–12; curve bend must be finite and within its supported range".into()));
        }
        Ok(())
    }
    pub fn changed_fields(self, before: Self, after: Self) -> Self {
        let mut out = self;
        macro_rules! copy {($($field:ident),*)=>{$(if before.$field != after.$field {out.$field=after.$field;})*};}
        copy!(start, end, start_size, end_size, pattern, curve, bend);
        if out.curve == Curve::Circular {
            out.bend = out.bend.clamp(-0.5, 0.5);
        }
        out
    }
}
pub struct Head {
    pub path: BezPath,
    pub outline: Option<f64>,
}

pub struct Geometry {
    pub shaft: BezPath,
    pub full_shaft: BezPath,
    pub heads: Vec<Head>,
}
fn p(point: [f32; 2]) -> Point {
    Point::new(point[0].into(), point[1].into())
}
fn polygon(points: &[(f64, f64)], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    path.move_to(points[0]);
    for point in &points[1..] {
        path.line_to(*point);
    }
    if closed {
        path.close_path();
    }
    path
}
fn endpoint(kind: Endpoint, width: f64, size: f64) -> Option<(BezPath, Option<f64>, f64)> {
    let k = width.max(0.5) * size / 3.;
    Some(match kind {
        Endpoint::Flat => return None,
        Endpoint::Round => (Circle::new((0., 0.), width * 0.5).to_path(0.01), None, 0.),
        Endpoint::OpenArrow => (
            polygon(&[(-4. * k, 2.5 * k), (0., 0.), (-4. * k, -2.5 * k)], false),
            Some(width),
            0.,
        ),
        Endpoint::Triangle => (
            polygon(&[(0., 0.), (-4.5 * k, 2. * k), (-4.5 * k, -2. * k)], true),
            None,
            4. * k,
        ),
        Endpoint::WideArrow => (
            polygon(&[(0., 0.), (-4.5 * k, 3.2 * k), (-4.5 * k, -3.2 * k)], true),
            None,
            4. * k,
        ),
        Endpoint::Stealth => (
            polygon(
                &[
                    (0., 0.),
                    (-5. * k, 2.6 * k),
                    (-3.4 * k, 0.),
                    (-5. * k, -2.6 * k),
                ],
                true,
            ),
            None,
            3.4 * k,
        ),
        Endpoint::Circle => (Circle::new((0., 0.), 2.2 * k).to_path(0.01), None, 0.),
        Endpoint::Square => (
            polygon(
                &[
                    (-2. * k, -2. * k),
                    (2. * k, -2. * k),
                    (2. * k, 2. * k),
                    (-2. * k, 2. * k),
                ],
                true,
            ),
            None,
            0.,
        ),
        Endpoint::Diamond => (
            polygon(
                &[(-2.8 * k, 0.), (0., -2.8 * k), (2.8 * k, 0.), (0., 2.8 * k)],
                true,
            ),
            None,
            0.,
        ),
        Endpoint::Bar => (
            polygon(
                &[
                    (-0.5 * k, -3. * k),
                    (0.5 * k, -3. * k),
                    (0.5 * k, 3. * k),
                    (-0.5 * k, 3. * k),
                ],
                true,
            ),
            None,
            0.,
        ),
    })
}
fn direction(seg: PathSeg, start: bool) -> Vec2 {
    let t = if start { 0. } else { 1. };
    let mut d = match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(t).to_vec2(),
        PathSeg::Cubic(c) => c.deriv().eval(t).to_vec2(),
    };
    if d.hypot() < 1e-9 {
        d = seg.end() - seg.start();
    }
    if start {
        d = -d;
    }
    if d.hypot() < 1e-9 {
        Vec2::new(1., 0.)
    } else {
        d.normalize()
    }
}
fn shaft(a: &Annotation, style: Style) -> BezPath {
    let start = p(a.start);
    let end = p(a.end);
    let delta = end - start;
    let length = delta.hypot();
    let mut path = BezPath::new();
    path.move_to(start);
    if length < 0.001 || style.curve == Curve::Straight || style.bend.abs() < 0.0001 {
        path.line_to(end);
        return path;
    }
    let normal = Vec2::new(-delta.y, delta.x) / length;
    let mid = start + delta * 0.5;
    let sag = f64::from(style.bend) * length;
    if style.curve == Curve::Bezier {
        path.quad_to(mid + normal * (sag * 2.), end);
        return path;
    }
    let center = mid + normal * ((sag * sag - length * length * 0.25) / (2. * sag));
    let radius = (start - center).hypot();
    let start_angle = (start - center).atan2();
    Arc::new(
        center,
        (radius, radius),
        start_angle,
        -4. * (2. * f64::from(style.bend)).atan(),
        0.,
    )
    .to_path(0.001)
}
pub fn geometry(a: &Annotation) -> Geometry {
    let style = a.line_style.unwrap_or_else(|| Style::for_kind(a.kind));
    let mut full = shaft(a, style);
    if let Some(progress) = style.reveal {
        let length = full.segments().map(|s| s.arclen(1e-3)).sum::<f64>();
        let mut remaining = length * progress.clamp(0., 1.);
        let mut skip = if style.reveal_from_end {
            length - remaining
        } else {
            0.
        };
        let mut partial = BezPath::new();
        for segment in full.segments() {
            if remaining <= 0. {
                break;
            }
            let length = segment.arclen(1e-3);
            if skip >= length {
                skip -= length;
                continue;
            }
            let start = segment.inv_arclen(skip, 1e-3);
            let end = segment.inv_arclen((skip + remaining).min(length), 1e-3);
            let segment = segment.subsegment(start..end);
            if partial.is_empty() {
                partial.move_to(segment.start());
            }
            match segment {
                PathSeg::Line(l) => partial.line_to(l.p1),
                PathSeg::Quad(q) => partial.quad_to(q.p1, q.p2),
                PathSeg::Cubic(c) => partial.curve_to(c.p1, c.p2, c.p3),
            }
            remaining -= length - skip;
            skip = 0.;
        }
        full = partial;
    }
    if a.line_style.is_none() {
        let heads = if a.kind == Kind::Arrow {
            a.arrow_head()
                .map(|w| Head {
                    path: polygon(
                        &[
                            (w[0][0].into(), w[0][1].into()),
                            (a.end[0].into(), a.end[1].into()),
                            (w[1][0].into(), w[1][1].into()),
                        ],
                        false,
                    ),
                    outline: Some(a.width.into()),
                })
                .into_iter()
                .collect()
        } else {
            vec![]
        };
        return Geometry {
            shaft: full.clone(),
            full_shaft: full,
            heads,
        };
    }
    let mut segments: Vec<_> = full.segments().collect();
    let mut heads = vec![];
    if !segments.is_empty() {
        // Orient from untrimmed tangents, then trim each end without moving tips.
        for (kind, size, start) in [
            (style.start, style.start_size, true),
            (style.end, style.end_size, false),
        ] {
            let Some((path, outline, pull)) = endpoint(kind, a.width.into(), size.into()) else {
                continue;
            };
            let original = if start {
                full.segments().next().unwrap()
            } else {
                full.segments().last().unwrap()
            };
            let tip = if start {
                original.start()
            } else {
                original.end()
            };
            let dir = direction(original, start);
            heads.push(Head {
                path: (Affine::translate(tip.to_vec2()) * Affine::rotate(dir.atan2())) * path,
                outline,
            });
            let index = if start { 0 } else { segments.len() - 1 };
            let seg = segments[index];
            let len = seg.arclen(1e-3);
            if pull > 0. && len > 1e-9 {
                let trim = pull.min(len * 0.45);
                segments[index] = if start {
                    seg.subsegment(seg.inv_arclen(trim, 1e-3)..1.)
                } else {
                    seg.subsegment(0.0..seg.inv_arclen(len - trim, 1e-3))
                };
            }
        }
    }
    let mut path = BezPath::new();
    for (i, seg) in segments.iter().enumerate() {
        if i == 0 {
            path.move_to(seg.start());
        }
        match seg {
            PathSeg::Line(l) => path.line_to(l.p1),
            PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
            PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
        }
    }
    Geometry {
        shaft: style.pattern.path(&path, a.width.into()),
        full_shaft: style.pattern.path(&full, a.width.into()),
        heads,
    }
}
pub fn bend_point(a: &Annotation) -> [f32; 2] {
    let delta = [a.end[0] - a.start[0], a.end[1] - a.start[1]];
    let bend = a
        .line_style
        .filter(|s| s.curve != Curve::Straight)
        .map_or(0., |s| s.bend);
    [
        (a.start[0] + a.end[0]) * 0.5 - delta[1] * bend,
        (a.start[1] + a.end[1]) * 0.5 + delta[0] * bend,
    ]
}
pub fn bend_to(a: &Annotation, point: [f32; 2]) -> Annotation {
    let mut out = a.clone();
    let mut style = a.line_style.unwrap_or_else(|| Style::for_kind(a.kind));
    let delta = [a.end[0] - a.start[0], a.end[1] - a.start[1]];
    let length2 = delta[0] * delta[0] + delta[1] * delta[1];
    if length2 > 0.001 {
        if style.curve == Curve::Straight {
            style.curve = Curve::Circular;
        }
        let limit = if style.curve == Curve::Circular {
            0.5
        } else {
            2.
        };
        let mid = [(a.start[0] + a.end[0]) * 0.5, (a.start[1] + a.end[1]) * 0.5];
        style.bend = (-(point[0] - mid[0]) * delta[1] + (point[1] - mid[1]) * delta[0]) / length2;
        style.bend = style.bend.clamp(-limit, limit);
        out.line_style = Some(style);
    }
    out
}
pub fn bounds(a: &Annotation) -> ([f32; 2], [f32; 2]) {
    let g = geometry(a);
    let mut bounds = g.full_shaft.bounding_box();
    for head in g.heads {
        bounds = bounds.union(head.path.bounding_box().inflate(
            head.outline.unwrap_or(0.) * 0.5,
            head.outline.unwrap_or(0.) * 0.5,
        ));
    }
    (
        [bounds.x0 as f32, bounds.y0 as f32],
        [bounds.x1 as f32, bounds.y1 as f32],
    )
}
pub fn hit(a: &Annotation, point: [f32; 2], tolerance: f32) -> bool {
    let g = geometry(a);
    let p = p(point);
    let near = |path: &BezPath, reach: f64| {
        path.segments()
            .any(|s| s.nearest(p, 0.01).distance_sq <= reach * reach)
    };
    near(&g.full_shaft, tolerance.into())
        || g.heads.iter().any(|h| {
            (h.outline.is_none() && h.path.winding(p) != 0)
                || near(
                    &h.path,
                    f64::from(tolerance) + h.outline.unwrap_or(0.) * 0.5,
                )
        })
}
pub fn outlines(a: &Annotation, tolerance: f64) -> Vec<Vec<[f32; 2]>> {
    let g = geometry(a);
    let mut lines = vec![];
    for path in std::iter::once(g.full_shaft).chain(g.heads.into_iter().map(|h| h.path)) {
        let mut points = vec![];
        vello_cpu::kurbo::flatten(path.iter(), tolerance.max(0.01), |el| match el {
            PathEl::MoveTo(p) => {
                if !points.is_empty() {
                    lines.push(std::mem::take(&mut points));
                }
                points.push([p.x as f32, p.y as f32]);
            }
            PathEl::LineTo(p) => points.push([p.x as f32, p.y as f32]),
            PathEl::ClosePath => {
                if let Some(first) = points.first().copied() {
                    points.push(first);
                }
            }
            _ => {}
        });
        if !points.is_empty() {
            lines.push(points);
        }
    }
    lines
}

#[cfg(test)]
mod tests;
