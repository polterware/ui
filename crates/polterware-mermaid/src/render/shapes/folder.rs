//! Upstream `shapes/folder.ts`: a raised-tab folder with the `basic label-container` role.
use super::{IntersectKind, Node, RenderedNode, ShapeContext, ShapeError, prepare, shape_style};
use crate::scene::{Item, Path, PathShape, Point, Rect, Size, SourceRef};

pub fn draw(node: &Node, ctx: &ShapeContext<'_>) -> Result<RenderedNode, ShapeError> {
    let mut node=node.clone();
    let (mut group,label,text)=prepare(&node,ctx)?;
    let padding=if node.padding==0.0 {12.0} else {node.padding};
    let w=(text.width+padding*2.0).max(node.width.unwrap_or(0.0)).max(90.0);
    let content_h=text.height+padding*2.0;
    let tab_h=(content_h*0.16).clamp(8.0,14.0);
    let total_h=(content_h+tab_h).max(node.height.unwrap_or(0.0));
    let tab_w=(w*0.38).max(28.0); let top=-total_h/2.0;
    let pts=[Point::new(-w/2.0,top),Point::new(-w/2.0+tab_w,top),Point::new(-w/2.0+tab_w,top+tab_h),Point::new(w/2.0,top+tab_h),Point::new(w/2.0,total_h/2.0),Point::new(-w/2.0,total_h/2.0)];
    let mut path=Path::new(); path.move_to(pts[0]); for p in pts.iter().skip(1) {path.line_to(*p);} path.close();
    // handDrawn: rough.js (later)
    group.push(PathShape::new(path,shape_style()).with_source(SourceRef::node(node.id.clone())));
    node.bounds=Rect::centered(Point::ZERO,Size::new(w,total_h));
    node.intersect=IntersectKind::Polygon(pts.to_vec());
    group.items.push(Item::Group(label));
    Ok(RenderedNode{node,group})
}
