//! Backend-neutral vector coverage shared by the viewport and PDF exporter.
use vello_cpu::{
    RenderContext,
    color::{AlphaColor, Srgb},
    kurbo::{BezPath, Rect, Shape, Stroke},
    peniko::BlendMode,
};

pub trait Painter {
    fn set_stroke(&mut self, stroke: Stroke);
    fn stroke_path(&mut self, path: &BezPath);
    fn fill_path(&mut self, path: &BezPath);
    fn fill_rect(&mut self, rect: &Rect);
    fn push_clip_path(&mut self, path: &BezPath);
    fn pop_clip_path(&mut self);
    fn push_blend_layer(&mut self, blend: BlendMode);
    fn pop_layer(&mut self);
    fn set_paint(&mut self, color: AlphaColor<Srgb>);
}
impl Painter for RenderContext {
    fn set_stroke(&mut self, s: Stroke) {
        RenderContext::set_stroke(self, s);
    }
    fn stroke_path(&mut self, p: &BezPath) {
        RenderContext::stroke_path(self, p);
    }
    fn fill_path(&mut self, p: &BezPath) {
        RenderContext::fill_path(self, p);
    }
    fn fill_rect(&mut self, p: &Rect) {
        RenderContext::fill_rect(self, p);
    }
    fn push_clip_path(&mut self, p: &BezPath) {
        RenderContext::push_clip_path(self, p);
    }
    fn pop_clip_path(&mut self) {
        RenderContext::pop_clip_path(self);
    }
    fn push_blend_layer(&mut self, b: BlendMode) {
        RenderContext::push_blend_layer(self, b);
    }
    fn pop_layer(&mut self) {
        RenderContext::pop_layer(self);
    }
    fn set_paint(&mut self, c: AlphaColor<Srgb>) {
        RenderContext::set_paint(self, c);
    }
}
#[derive(Clone)]
pub(crate) enum Op {
    Fill(BezPath),
    Stroke(BezPath, Stroke),
    Clip(BezPath),
    PopClip,
    Layer(BlendMode),
    PopLayer,
}
#[derive(Default)]
pub(crate) struct Recorder {
    pub ops: Vec<Op>,
    stroke: Stroke,
}
impl Painter for Recorder {
    fn set_stroke(&mut self, s: Stroke) {
        self.stroke = s;
    }
    fn stroke_path(&mut self, p: &BezPath) {
        self.ops.push(Op::Stroke(p.clone(), self.stroke.clone()));
    }
    fn fill_path(&mut self, p: &BezPath) {
        self.ops.push(Op::Fill(p.clone()));
    }
    fn fill_rect(&mut self, p: &Rect) {
        self.fill_path(&p.to_path(0.01));
    }
    fn push_clip_path(&mut self, p: &BezPath) {
        self.ops.push(Op::Clip(p.clone()));
    }
    fn pop_clip_path(&mut self) {
        self.ops.push(Op::PopClip);
    }
    fn push_blend_layer(&mut self, b: BlendMode) {
        self.ops.push(Op::Layer(b));
    }
    fn pop_layer(&mut self) {
        self.ops.push(Op::PopLayer);
    }
    fn set_paint(&mut self, _: AlphaColor<Srgb>) {}
}
pub(crate) fn record(a: &crate::Annotation) -> Vec<(Vec<Op>, [u8; 4])> {
    let mut result = vec![];
    super::viewport::paint_annotation(&mut Recorder::default(), a, 1., &mut |r, ink| {
        if ink[3] > 0 {
            result.push((std::mem::take(&mut r.ops), ink));
        } else {
            r.ops.clear();
        }
    });
    result
}
