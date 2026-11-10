//! Focus uses retained lower/affected surfaces and a small moving mask mesh.
//! Moving, resizing or rotating its window never resamples the canvas pixels.
use super::*;
pub(super) type Tiles = Vec<([u32; 4], egui::TextureHandle)>;

pub(super) fn textures(ui: &egui::Ui, pixels: &image::RgbaImage, limit: u32) -> Tiles {
    let mut tiles = vec![];
    for y in (0..pixels.height()).step_by(limit as usize) {
        for x in (0..pixels.width()).step_by(limit as usize) {
            let w = limit.min(pixels.width() - x);
            let h = limit.min(pixels.height() - y);
            let patch = image::imageops::crop_imm(pixels, x, y, w, h).to_image();
            tiles.push((
                [x, y, w, h],
                ui.ctx().load_texture(
                    "focus-clear",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [w as usize, h as usize],
                        patch.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                ),
            ));
        }
    }
    tiles
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    painter: &egui::Painter,
    t: Transform,
    a: &Annotation,
    output: [u32; 4],
    clear: &Tiles,
    affected: &Tiles,
    holes: &[egui::Rect],
    density: f32,
    detail: Option<&([u32; 4], Tiles)>,
    others: &[Annotation],
) {
    let boundary: Vec<_> = a
        .frame_boundary()
        .into_iter()
        .map(|p| a.rotation().point(p))
        .collect();
    if boundary.len() < 3 {
        return;
    }
    let (lo, hi) = a.body_bounds();
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    // Scale the convex contour until it encloses the whole output. Using its
    // shortest half-axis also covers thin windows and windows off the page.
    let reach = (center[0] - output[0] as f32).abs()
        + (center[1] - output[1] as f32).abs()
        + output[2] as f32
        + output[3] as f32
        + 4.;
    let factor = reach / ((hi[0] - lo[0]).min(hi[1] - lo[1]) * 0.25).max(0.001) + 2.;
    for (outside, tiles) in [(false, clear), (true, affected)] {
        for ([x, y, w, h], texture) in tiles {
            let origin = [
                output[0] as f32 + *x as f32 / density,
                output[1] as f32 + *y as f32 / density,
            ];
            let rect = egui::Rect::from_min_size(
                t.screen_point(origin),
                egui::vec2(*w as f32, *h as f32) * (t.scale / density),
            );
            if !rect.intersects(t.viewport) {
                continue;
            }
            let mut mesh = egui::Mesh::with_texture(texture.id());
            let vertex = |p: [f32; 2]| egui::epaint::Vertex {
                pos: t.screen_point(p),
                uv: egui::pos2(
                    (p[0] - origin[0]) * density / *w as f32,
                    (p[1] - origin[1]) * density / *h as f32,
                ),
                color: egui::Color32::WHITE,
            };
            if outside {
                for p in &boundary {
                    mesh.vertices.push(vertex(*p));
                    mesh.vertices.push(vertex([
                        center[0] + (p[0] - center[0]) * factor,
                        center[1] + (p[1] - center[1]) * factor,
                    ]));
                }
                for i in 0..boundary.len() as u32 {
                    let n = (i + 1) % boundary.len() as u32;
                    mesh.indices
                        .extend([i * 2, n * 2, i * 2 + 1, n * 2, n * 2 + 1, i * 2 + 1]);
                }
            } else {
                mesh.vertices.extend(boundary.iter().copied().map(vertex));
                for i in 1..boundary.len() as u32 - 1 {
                    mesh.indices.extend([0, i, i + 1]);
                }
            }
            // Keep positions/UVs bounded before GPU interpolation. Long ring
            // triangles otherwise magnify floating-point error on detailed
            // screenshots and create visible radial seams at fractional zoom.
            let mesh = clipped_mesh(&mesh, rect);
            with_holes(painter, rect, holes, |p| {
                p.add(mesh.clone());
            });
        }
    }
    // Windows of the other Focus areas sharing this backdrop stay clear, drawn
    // from live geometry so moving one uploads nothing.
    for other in others {
        let shape: Vec<_> = other
            .frame_boundary()
            .into_iter()
            .map(|p| other.rotation().point(p))
            .collect();
        if shape.len() < 3 {
            continue;
        }
        for ([x, y, w, h], texture) in clear {
            let origin = [
                output[0] as f32 + *x as f32 / density,
                output[1] as f32 + *y as f32 / density,
            ];
            let rect = egui::Rect::from_min_size(
                t.screen_point(origin),
                egui::vec2(*w as f32, *h as f32) * (t.scale / density),
            );
            if !rect.intersects(t.viewport) {
                continue;
            }
            let mut mesh = egui::Mesh::with_texture(texture.id());
            mesh.vertices
                .extend(shape.iter().map(|p| egui::epaint::Vertex {
                    pos: t.screen_point(*p),
                    uv: egui::Pos2::ZERO,
                    color: egui::Color32::WHITE,
                }));
            for i in 1..shape.len() as u32 - 1 {
                mesh.indices.extend([0, i, i + 1]);
            }
            let mesh = clipped_mesh(&mesh, rect);
            with_holes(painter, rect, holes, |p| {
                p.add(mesh.clone());
            });
        }
    }
    if let Some((region, tiles)) = detail {
        paint_lens(painter, t, a, *region, tiles, holes, 1.);
    } else {
        paint_lens(painter, t, a, output, clear, holes, density);
    }
}

/// A Focus sharing another's backdrop draws only its Magnify view, read from
/// the lead Focus's undimmed picture.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_member(
    painter: &egui::Painter,
    t: Transform,
    a: &Annotation,
    output: [u32; 4],
    clear: &Tiles,
    holes: &[egui::Rect],
    density: f32,
    detail: Option<&([u32; 4], Tiles)>,
) {
    if let Some((region, tiles)) = detail {
        paint_lens(painter, t, a, *region, tiles, holes, 1.);
    } else {
        paint_lens(painter, t, a, output, clear, holes, density);
    }
}

fn paint_lens(
    painter: &egui::Painter,
    t: Transform,
    a: &Annotation,
    output: [u32; 4],
    clear: &Tiles,
    holes: &[egui::Rect],
    density: f32,
) {
    let Some(b) = capture_core::magnified::lens(a) else {
        return;
    };
    let (lo, hi) = a.body_bounds();
    let (blo, bhi) = b.body_bounds();
    let map = |p: [f32; 2]| {
        let p = a.rotation().inverse_point(p);
        b.rotation().point([
            blo[0] + (p[0] - lo[0]) / (hi[0] - lo[0]).max(0.001) * (bhi[0] - blo[0]),
            blo[1] + (p[1] - lo[1]) / (hi[1] - lo[1]).max(0.001) * (bhi[1] - blo[1]),
        ])
    };
    let opacity = a.focus.unwrap().opacity;
    let boundary: Vec<_> = a
        .frame_boundary()
        .into_iter()
        .map(|p| a.rotation().point(p))
        .collect();
    let stroke = egui::Stroke::new(
        a.width as f32 * t.scale,
        egui::Color32::from_rgba_unmultiplied(a.color[0], a.color[1], a.color[2], a.color[3]),
    );
    with_holes(painter, t.viewport, holes, |p| {
        for line in capture_core::magnified::connectors(a) {
            paint_stroke(p, t, &line, stroke);
        }
        for ([x, y, w, h], texture) in clear {
            let rect = egui::Rect::from_min_size(
                egui::pos2(
                    output[0] as f32 + *x as f32 / density,
                    output[1] as f32 + *y as f32 / density,
                ),
                egui::vec2(*w as f32, *h as f32) / density,
            );
            let mut mesh = egui::Mesh::with_texture(texture.id());
            mesh.vertices
                .extend(boundary.iter().map(|v| egui::epaint::Vertex {
                    pos: egui::pos2(v[0], v[1]),
                    uv: egui::Pos2::ZERO,
                    color: egui::Color32::WHITE,
                }));
            for i in 1..boundary.len() as u32 - 1 {
                mesh.indices.extend([0, i, i + 1]);
            }
            let mut mesh = clipped_mesh(&mesh, rect);
            for v in &mut mesh.vertices {
                v.pos = t.screen_point(map([v.pos.x, v.pos.y]));
                v.color = egui::Color32::WHITE.gamma_multiply(opacity);
            }
            p.add(mesh);
        }
        paint_stroke(p, t, &b, stroke);
    });
}

fn paint_stroke(p: &egui::Painter, t: Transform, a: &Annotation, stroke: egui::Stroke) {
    for path in capture_core::magnified::stroke_paths(a, 0.2 / t.scale.max(0.01) as f64) {
        p.add(egui::Shape::line(
            path.into_iter().map(|v| t.screen_point(v)).collect(),
            stroke,
        ));
    }
}

fn clipped_mesh(source: &egui::Mesh, rect: egui::Rect) -> egui::Mesh {
    let mut out = egui::Mesh::with_texture(source.texture_id);
    for triangle in source.indices.as_chunks::<3>().0 {
        let mut polygon: Vec<_> = triangle
            .iter()
            .map(|i| source.vertices[*i as usize].pos)
            .collect();
        for (axis, edge, lower) in [
            (0, rect.left(), true),
            (0, rect.right(), false),
            (1, rect.top(), true),
            (1, rect.bottom(), false),
        ] {
            if polygon.is_empty() {
                break;
            }
            let inside = |p: egui::Pos2| {
                if lower {
                    p[axis] >= edge
                } else {
                    p[axis] <= edge
                }
            };
            let mut next = vec![];
            let mut a = *polygon.last().unwrap();
            for b in polygon {
                if inside(a) != inside(b) {
                    let t = (edge - a[axis]) / (b[axis] - a[axis]);
                    let mut point = a + (b - a) * t;
                    point[axis] = edge;
                    next.push(point);
                }
                if inside(b) {
                    next.push(b);
                }
                a = b;
            }
            polygon = next;
        }
        if polygon.len() < 3 {
            continue;
        }
        let start = out.vertices.len() as u32;
        for p in &polygon {
            out.vertices.push(egui::epaint::Vertex {
                pos: *p,
                uv: egui::pos2(
                    (p.x - rect.left()) / rect.width(),
                    (p.y - rect.top()) / rect.height(),
                ),
                color: egui::Color32::WHITE,
            });
        }
        for i in 1..polygon.len() as u32 - 1 {
            out.indices.extend([start, start + i, start + i + 1]);
        }
    }
    out
}
