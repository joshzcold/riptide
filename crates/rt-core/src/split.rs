//! Split panes: a window's page area as a tree of panes, each showing one
//! tab. The browser layer gives each pane a view; the tree, the focused pane
//! and moving between panes live here, so they're tested without CEF.

/// A pane's id within its window. Ids aren't reused while the window is open.
pub type PaneId = u32;

/// How a split arranges its panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// One above the other (`:split`).
    Horizontal,
    /// Side by side (`:vsplit`).
    Vertical,
}

/// Where `:split-focus` moves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

impl Direction {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "left" => Self::Left,
            "down" => Self::Down,
            "up" => Self::Up,
            "right" => Self::Right,
            _ => return None,
        })
    }
}

/// A pane's place in the window, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Pane(PaneId),
    /// Children with their share of the split, adding up to 1.
    Split {
        axis: Axis,
        children: Vec<(Node, f64)>,
    },
}

/// A pane's place as fractions of the page area.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

const EPSILON: f64 = 1e-9;

/// A window's panes. A new layout has one pane, `0`.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    root: Node,
    focused: PaneId,
    next_id: PaneId,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    pub fn new() -> Self {
        Self {
            root: Node::Pane(0),
            focused: 0,
            next_id: 1,
        }
    }

    pub fn focused(&self) -> PaneId {
        self.focused
    }

    /// Whether there's more than one pane.
    pub fn is_split(&self) -> bool {
        matches!(self.root, Node::Split { .. })
    }

    /// Every pane, top-left first.
    pub fn panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        collect(&self.root, &mut panes);
        panes
    }

    pub fn contains(&self, pane: PaneId) -> bool {
        self.panes().contains(&pane)
    }

    /// Focus `pane`; false if there's no such pane.
    pub fn focus(&mut self, pane: PaneId) -> bool {
        let found = self.contains(pane);
        if found {
            self.focused = pane;
        }
        found
    }

    /// Split the focused pane in two along `axis`. The new pane comes after
    /// it (below or to the right), takes half its room, and is focused.
    pub fn split(&mut self, axis: Axis) -> PaneId {
        let new = self.next_id;
        self.next_id += 1;
        split_node(&mut self.root, self.focused, new, axis);
        self.focused = new;
        new
    }

    /// Close `pane`, giving its room to its neighbours. The last pane can't
    /// be closed. If it was focused, the pane before it in its split (or
    /// after, for the first) is focused instead.
    pub fn close(&mut self, pane: PaneId) -> bool {
        if !self.is_split() || !self.contains(pane) {
            return false;
        }
        let neighbour = remove_node(&mut self.root, pane);
        collapse(&mut self.root);
        if self.focused == pane {
            self.focused = neighbour
                .filter(|p| self.contains(*p))
                .unwrap_or_else(|| self.panes()[0]);
        }
        true
    }

    /// Keep only the focused pane; returns the panes that went.
    pub fn only(&mut self) -> Vec<PaneId> {
        let gone: Vec<PaneId> = self
            .panes()
            .into_iter()
            .filter(|&p| p != self.focused)
            .collect();
        self.root = Node::Pane(self.focused);
        gone
    }

    /// The pane after the focused one, wrapping around.
    pub fn next(&self) -> PaneId {
        let panes = self.panes();
        let i = panes.iter().position(|&p| p == self.focused).unwrap_or(0);
        panes[(i + 1) % panes.len()]
    }

    /// The pane next to the focused one in `direction`: of the panes that
    /// touch its edge, the one level with its middle, or else the closest.
    pub fn neighbour(&self, direction: Direction) -> Option<PaneId> {
        let rects = self.rects();
        let (_, here) = *rects.iter().find(|(p, _)| *p == self.focused)?;
        let (cx, cy) = here.center();
        rects
            .iter()
            .filter(|(p, _)| *p != self.focused)
            .filter_map(|&(pane, r)| {
                // How far the edges are apart, and how far the other pane's
                // span is from this one's middle (0 when it's level with it).
                let (gap, along, overlap) = match direction {
                    Direction::Left => (
                        here.x - (r.x + r.width),
                        span_distance(cy, r.y, r.height),
                        overlap(here.y, here.height, r.y, r.height),
                    ),
                    Direction::Right => (
                        r.x - (here.x + here.width),
                        span_distance(cy, r.y, r.height),
                        overlap(here.y, here.height, r.y, r.height),
                    ),
                    Direction::Up => (
                        here.y - (r.y + r.height),
                        span_distance(cx, r.x, r.width),
                        overlap(here.x, here.width, r.x, r.width),
                    ),
                    Direction::Down => (
                        r.y - (here.y + here.height),
                        span_distance(cx, r.x, r.width),
                        overlap(here.x, here.width, r.x, r.width),
                    ),
                };
                (gap.abs() < EPSILON && overlap > EPSILON).then_some((pane, along))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(pane, _)| pane)
    }

    /// Give every pane in each split the same room.
    pub fn equalize(&mut self) {
        equalize(&mut self.root);
    }

    /// Grow the focused pane along `axis` (`Vertical`: wider) by `delta` of
    /// its split's room, taken from the pane beside it; negative shrinks it.
    /// False if no split runs along `axis` around it.
    pub fn resize(&mut self, axis: Axis, delta: f64) -> bool {
        resize(&mut self.root, self.focused, axis, delta).unwrap_or(false)
    }

    /// The borders between neighbouring panes in a `width` × `height` page
    /// area, each `grab` pixels across, centred on the gap between them.
    pub fn borders(&self, width: i32, height: i32, grab: i32) -> Vec<Border> {
        let mut out = Vec::new();
        let full = Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        let size = (f64::from(width), f64::from(height));
        borders(&self.root, full, &mut Vec::new(), size, grab, &mut out);
        out
    }

    /// Move the border `id` by `delta` pixels (right or down) in a split
    /// `extent` pixels across. False if there's no such border.
    pub fn move_border(&mut self, id: &str, delta: i32, extent: i32) -> bool {
        let Some((path, after)) = parse_border(id) else {
            return false;
        };
        let mut node = &mut self.root;
        for i in path {
            match node {
                Node::Split { children, .. } if i < children.len() => node = &mut children[i].0,
                _ => return false,
            }
        }
        let Node::Split { children, .. } = node else {
            return false;
        };
        if after + 1 >= children.len() || extent <= 0 {
            return false;
        }
        let total = children[after].1 + children[after + 1].1;
        let moved = (children[after].1 + f64::from(delta) / f64::from(extent))
            .clamp(MIN_SHARE, total - MIN_SHARE);
        children[after].1 = moved;
        children[after + 1].1 = total - moved;
        true
    }

    /// The layout as a tree of whatever `tab` gives each pane, for saving.
    /// Panes it gives nothing for are left out; `None` if one pane or less is left.
    pub fn tree<T>(&self, mut tab: impl FnMut(PaneId) -> Option<T>) -> Option<Tree<T>> {
        let tree = to_tree(&self.root, &mut tab)?;
        matches!(tree, Tree::Split { .. }).then_some(tree)
    }

    /// A layout from a saved tree, with the pane for each item; the first
    /// pane for which `focus` holds is focused.
    pub fn from_tree<T>(tree: Tree<T>, focus: impl Fn(&T) -> bool) -> (Self, Vec<(PaneId, T)>) {
        let mut panes = Vec::new();
        let mut next_id = 0;
        let root = from_tree(tree, &mut next_id, &mut panes);
        let mut layout = Self {
            root,
            focused: 0,
            next_id,
        };
        collapse(&mut layout.root);
        if let Some((pane, _)) = panes.iter().find(|(_, t)| focus(t)) {
            layout.focused = *pane;
        }
        (layout, panes)
    }

    /// Each pane's place in a `width` × `height` page area, with `gap`
    /// pixels between neighbouring panes and none at the outer edges.
    pub fn bounds(&self, width: i32, height: i32, gap: i32) -> Vec<(PaneId, Bounds)> {
        let (w, h) = (f64::from(width), f64::from(height));
        let half = f64::from(gap) / 2.0;
        self.rects()
            .into_iter()
            .map(|(pane, r)| {
                // Shrink only edges inside the area, so neighbours split the gap.
                let inner = |edge: f64| {
                    if edge > EPSILON && edge < 1.0 - EPSILON {
                        half
                    } else {
                        0.0
                    }
                };
                let left = r.x * w + inner(r.x);
                let right = (r.x + r.width) * w - inner(r.x + r.width);
                let top = r.y * h + inner(r.y);
                let bottom = (r.y + r.height) * h - inner(r.y + r.height);
                let (x, y) = (left.round() as i32, top.round() as i32);
                (
                    pane,
                    Bounds {
                        x,
                        y,
                        width: (right.round() as i32 - x).max(0),
                        height: (bottom.round() as i32 - y).max(0),
                    },
                )
            })
            .collect()
    }

    fn rects(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        place(
            &self.root,
            Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            &mut out,
        );
        out
    }
}

/// A layout to save and restore, with something (such as a tab's index) in
/// each pane. Shares add up to 1 in each split.
#[derive(Clone, Debug, PartialEq)]
pub enum Tree<T> {
    Pane(T),
    Split {
        axis: Axis,
        children: Vec<(Tree<T>, f64)>,
    },
}

/// A border between two neighbouring panes, to drag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Border {
    /// Stays the same while the layout keeps its shape (see `move_border`).
    pub id: String,
    /// `Vertical` for a border between panes side by side, which moves left and right.
    pub axis: Axis,
    pub bounds: Bounds,
    /// How many pixels across its split is, along `axis`.
    pub extent: i32,
}

/// A border's id: the child indices down to its split, then the pane it follows.
pub fn border_id(path: &[usize], after: usize) -> String {
    let path: Vec<String> = path.iter().map(usize::to_string).collect();
    format!("{}:{after}", path.join("."))
}

fn parse_border(id: &str) -> Option<(Vec<usize>, usize)> {
    let (path, after) = id.split_once(':')?;
    let path = if path.is_empty() {
        Vec::new()
    } else {
        path.split('.')
            .map(|i| i.parse().ok())
            .collect::<Option<_>>()?
    };
    Some((path, after.parse().ok()?))
}

fn borders(
    node: &Node,
    rect: Rect,
    path: &mut Vec<usize>,
    (w, h): (f64, f64),
    grab: i32,
    out: &mut Vec<Border>,
) {
    let Node::Split { axis, children } = node else {
        return;
    };
    let mut offset = 0.0;
    for (i, (child, share)) in children.iter().enumerate() {
        let child_rect = match axis {
            Axis::Vertical => Rect {
                x: rect.x + offset * rect.width,
                width: share * rect.width,
                ..rect
            },
            Axis::Horizontal => Rect {
                y: rect.y + offset * rect.height,
                height: share * rect.height,
                ..rect
            },
        };
        path.push(i);
        borders(child, child_rect, path, (w, h), grab, out);
        path.pop();
        offset += share;
        if i + 1 == children.len() {
            continue;
        }
        let bounds = match axis {
            Axis::Vertical => Bounds {
                x: ((rect.x + offset * rect.width) * w).round() as i32 - grab / 2,
                y: (rect.y * h).round() as i32,
                width: grab,
                height: (rect.height * h).round() as i32,
            },
            Axis::Horizontal => Bounds {
                x: (rect.x * w).round() as i32,
                y: ((rect.y + offset * rect.height) * h).round() as i32 - grab / 2,
                width: (rect.width * w).round() as i32,
                height: grab,
            },
        };
        let extent = match axis {
            Axis::Vertical => rect.width * w,
            Axis::Horizontal => rect.height * h,
        };
        out.push(Border {
            id: border_id(path, i),
            axis: *axis,
            bounds,
            extent: extent.round() as i32,
        });
    }
}

/// The smallest share a resize leaves a pane.
const MIN_SHARE: f64 = 0.1;

/// `Some(done)` once the focused pane is found below `node`.
fn resize(node: &mut Node, focused: PaneId, axis: Axis, delta: f64) -> Option<bool> {
    let Node::Split {
        axis: split_axis,
        children,
    } = node
    else {
        return (*node == Node::Pane(focused)).then_some(false);
    };
    let i = children
        .iter_mut()
        .position(|(c, _)| contains(c, focused))?;
    // A split further down along the same axis gets it first.
    if let Some(true) = resize(&mut children[i].0, focused, axis, delta) {
        return Some(true);
    }
    if *split_axis != axis || children.len() < 2 {
        return Some(false);
    }
    let j = if i + 1 < children.len() { i + 1 } else { i - 1 };
    let total = children[i].1 + children[j].1;
    let grown = (children[i].1 + delta).clamp(MIN_SHARE, total - MIN_SHARE);
    children[i].1 = grown;
    children[j].1 = total - grown;
    Some(true)
}

fn contains(node: &Node, pane: PaneId) -> bool {
    match node {
        Node::Pane(p) => *p == pane,
        Node::Split { children, .. } => children.iter().any(|(c, _)| contains(c, pane)),
    }
}

fn to_tree<T>(node: &Node, tab: &mut impl FnMut(PaneId) -> Option<T>) -> Option<Tree<T>> {
    match node {
        Node::Pane(p) => tab(*p).map(Tree::Pane),
        Node::Split { axis, children } => {
            let mut kept: Vec<(Tree<T>, f64)> = children
                .iter()
                .filter_map(|(c, share)| Some((to_tree(c, tab)?, *share)))
                .collect();
            let total: f64 = kept.iter().map(|(_, s)| s).sum();
            for (_, s) in &mut kept {
                *s /= total;
            }
            match kept.len() {
                0 => None,
                1 => kept.pop().map(|(t, _)| t),
                _ => Some(Tree::Split {
                    axis: *axis,
                    children: kept,
                }),
            }
        }
    }
}

fn from_tree<T>(tree: Tree<T>, next_id: &mut PaneId, panes: &mut Vec<(PaneId, T)>) -> Node {
    match tree {
        Tree::Pane(item) => {
            let id = *next_id;
            *next_id += 1;
            panes.push((id, item));
            Node::Pane(id)
        }
        Tree::Split { axis, children } => {
            let total: f64 = children.iter().map(|(_, s)| s.max(0.0)).sum();
            let count = children.len() as f64;
            let children: Vec<(Node, f64)> = children
                .into_iter()
                .map(|(c, share)| {
                    // Saved shares that don't add up are made to, or shared out evenly.
                    let share = if total > EPSILON {
                        share.max(0.0) / total
                    } else {
                        1.0 / count
                    };
                    (from_tree(c, next_id, panes), share)
                })
                .collect();
            if children.is_empty() {
                let id = *next_id;
                *next_id += 1;
                return Node::Pane(id);
            }
            Node::Split { axis, children }
        }
    }
}

fn collect(node: &Node, out: &mut Vec<PaneId>) {
    match node {
        Node::Pane(p) => out.push(*p),
        Node::Split { children, .. } => children.iter().for_each(|(c, _)| collect(c, out)),
    }
}

fn place(node: &Node, rect: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match node {
        Node::Pane(p) => out.push((*p, rect)),
        Node::Split { axis, children } => {
            let mut offset = 0.0;
            for (child, share) in children {
                let r = match axis {
                    Axis::Vertical => Rect {
                        x: rect.x + offset * rect.width,
                        width: share * rect.width,
                        ..rect
                    },
                    Axis::Horizontal => Rect {
                        y: rect.y + offset * rect.height,
                        height: share * rect.height,
                        ..rect
                    },
                };
                place(child, r, out);
                offset += share;
            }
        }
    }
}

/// Split the pane `target` within `node`; true once it's found.
fn split_node(node: &mut Node, target: PaneId, new: PaneId, axis: Axis) -> bool {
    match node {
        Node::Pane(p) if *p == target => {
            *node = Node::Split {
                axis,
                children: vec![(Node::Pane(target), 0.5), (Node::Pane(new), 0.5)],
            };
            true
        }
        Node::Pane(_) => false,
        Node::Split {
            axis: split_axis,
            children,
        } => {
            // A pane in a split along the same axis gets a sibling, as in vim.
            if *split_axis == axis
                && let Some(i) = children.iter().position(|(c, _)| *c == Node::Pane(target))
            {
                let half = children[i].1 / 2.0;
                children[i].1 = half;
                children.insert(i + 1, (Node::Pane(new), half));
                return true;
            }
            children
                .iter_mut()
                .any(|(c, _)| split_node(c, target, new, axis))
        }
    }
}

/// Remove the pane `target`; returns the pane that should take focus.
fn remove_node(node: &mut Node, target: PaneId) -> Option<PaneId> {
    let Node::Split { children, .. } = node else {
        return None;
    };
    if let Some(i) = children.iter().position(|(c, _)| *c == Node::Pane(target)) {
        let (_, share) = children.remove(i);
        let rest: f64 = children.iter().map(|(_, s)| s).sum();
        for (_, s) in children.iter_mut() {
            *s += share * (*s / rest);
        }
        let neighbour = &children[i.saturating_sub(1).min(children.len() - 1)].0;
        return Some(first_pane(neighbour));
    }
    children
        .iter_mut()
        .find_map(|(c, _)| remove_node(c, target))
}

fn first_pane(node: &Node) -> PaneId {
    match node {
        Node::Pane(p) => *p,
        Node::Split { children, .. } => first_pane(&children[0].0),
    }
}

/// Replace splits left with one child by that child, and merge a split into
/// its parent when both run along the same axis.
fn collapse(node: &mut Node) {
    let Node::Split { axis, children } = node else {
        return;
    };
    for (child, _) in children.iter_mut() {
        collapse(child);
    }
    let axis = *axis;
    let mut merged = Vec::new();
    for (child, share) in std::mem::take(children) {
        match child {
            Node::Split {
                axis: a,
                children: inner,
            } if a == axis => merged.extend(inner.into_iter().map(|(c, s)| (c, s * share))),
            other => merged.push((other, share)),
        }
    }
    *children = merged;
    if children.len() == 1 {
        *node = children.remove(0).0;
    }
}

fn equalize(node: &mut Node) {
    if let Node::Split { children, .. } = node {
        let share = 1.0 / children.len() as f64;
        for (child, s) in children.iter_mut() {
            *s = share;
            equalize(child);
        }
    }
}

/// How far `point` is from the span `start..start + length` (0 inside it).
fn span_distance(point: f64, start: f64, length: f64) -> f64 {
    if point < start {
        start - point
    } else if point > start + length {
        point - (start + length)
    } else {
        0.0
    }
}

fn overlap(a: f64, a_len: f64, b: f64, b_len: f64) -> f64 {
    ((a + a_len).min(b + b_len) - a.max(b)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: i32, y: i32, width: i32, height: i32) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_new_layout_is_one_pane() {
        let layout = Layout::new();
        assert!(!layout.is_split());
        assert_eq!(layout.panes(), [0]);
        assert_eq!(layout.bounds(800, 600, 2), [(0, b(0, 0, 800, 600))]);
    }

    #[test]
    fn vsplit_puts_the_new_pane_on_the_right_and_focuses_it() {
        let mut layout = Layout::new();
        let right = layout.split(Axis::Vertical);
        assert_eq!(layout.focused(), right);
        assert_eq!(
            layout.bounds(800, 600, 2),
            [(0, b(0, 0, 399, 600)), (right, b(401, 0, 399, 600))]
        );
    }

    #[test]
    fn split_puts_the_new_pane_below() {
        let mut layout = Layout::new();
        let below = layout.split(Axis::Horizontal);
        assert_eq!(
            layout.bounds(800, 600, 0),
            [(0, b(0, 0, 800, 300)), (below, b(0, 300, 800, 300))]
        );
    }

    #[test]
    fn splitting_the_same_way_again_adds_a_sibling_with_half_the_room() {
        let mut layout = Layout::new();
        let second = layout.split(Axis::Vertical);
        let third = layout.split(Axis::Vertical);
        assert_eq!(
            layout.bounds(800, 600, 0),
            [
                (0, b(0, 0, 400, 600)),
                (second, b(400, 0, 200, 600)),
                (third, b(600, 0, 200, 600)),
            ]
        );
        layout.equalize();
        let widths: Vec<i32> = layout
            .bounds(900, 600, 0)
            .iter()
            .map(|(_, b)| b.width)
            .collect();
        assert_eq!(widths, [300, 300, 300]);
    }

    #[test]
    fn splitting_the_other_way_nests() {
        let mut layout = Layout::new();
        let right = layout.split(Axis::Vertical);
        let below_right = layout.split(Axis::Horizontal);
        assert_eq!(
            layout.bounds(800, 600, 0),
            [
                (0, b(0, 0, 400, 600)),
                (right, b(400, 0, 400, 300)),
                (below_right, b(400, 300, 400, 300)),
            ]
        );
    }

    #[test]
    fn hjkl_move_to_the_pane_beside_or_level_with_the_middle() {
        // +---+---+
        // |   | R |
        // | L +---+
        // |   | B |
        // +---+---+
        let mut layout = Layout::new();
        let r = layout.split(Axis::Vertical);
        let bottom = layout.split(Axis::Horizontal);
        assert_eq!(layout.neighbour(Direction::Up), Some(r));
        assert_eq!(layout.neighbour(Direction::Left), Some(0));
        assert_eq!(layout.neighbour(Direction::Down), None);
        assert_eq!(layout.neighbour(Direction::Right), None);
        layout.focus(0);
        // Both touch L's right edge and meet at its middle; the upper one wins.
        assert_eq!(layout.neighbour(Direction::Right), Some(r));
        layout.focus(r);
        assert_eq!(layout.neighbour(Direction::Down), Some(bottom));
        assert_eq!(layout.neighbour(Direction::Left), Some(0));
    }

    #[test]
    fn moving_sideways_picks_the_pane_level_with_the_middle() {
        // +---+---+
        // | A |   |
        // +---+   |
        // | B | C |
        // +---+   |
        // | D |   |
        // +---+---+
        let mut layout = Layout::new();
        let c = layout.split(Axis::Vertical);
        layout.focus(0);
        let b2 = layout.split(Axis::Horizontal);
        let d = layout.split(Axis::Horizontal);
        layout.equalize();
        layout.focus(c);
        assert_eq!(layout.neighbour(Direction::Left), Some(b2));
        layout.focus(d);
        assert_eq!(layout.neighbour(Direction::Right), Some(c));
        assert_eq!(layout.neighbour(Direction::Up), Some(b2));
    }

    #[test]
    fn next_cycles_through_the_panes() {
        let mut layout = Layout::new();
        let second = layout.split(Axis::Vertical);
        let third = layout.split(Axis::Horizontal);
        assert_eq!(layout.next(), 0);
        layout.focus(0);
        assert_eq!(layout.next(), second);
        layout.focus(second);
        assert_eq!(layout.next(), third);
    }

    #[test]
    fn closing_a_pane_gives_its_room_back_and_focuses_its_neighbour() {
        let mut layout = Layout::new();
        let right = layout.split(Axis::Vertical);
        assert!(layout.close(right));
        assert!(!layout.is_split());
        assert_eq!(layout.focused(), 0);
        assert_eq!(layout.bounds(800, 600, 2), [(0, b(0, 0, 800, 600))]);
        assert!(!layout.close(0), "the last pane stays");
    }

    #[test]
    fn closing_a_nested_pane_unnests_its_sibling() {
        let mut layout = Layout::new();
        let right = layout.split(Axis::Vertical);
        let below = layout.split(Axis::Horizontal);
        layout.close(right);
        assert_eq!(
            layout.focused(),
            below,
            "closing another pane keeps the focus"
        );
        assert_eq!(
            layout.bounds(800, 600, 0),
            [(0, b(0, 0, 400, 600)), (below, b(400, 0, 400, 600))]
        );
        // The split collapsed into one along the same axis, so a new vsplit is a sibling.
        layout.focus(below);
        layout.split(Axis::Vertical);
        assert_eq!(layout.bounds(800, 600, 0).len(), 3);
        assert!(
            layout
                .bounds(800, 600, 0)
                .iter()
                .all(|(_, b)| b.height == 600)
        );
    }

    #[test]
    fn closing_an_unfocused_pane_keeps_the_focus() {
        let mut layout = Layout::new();
        let second = layout.split(Axis::Vertical);
        layout.split(Axis::Vertical);
        layout.close(second);
        assert_eq!(layout.panes().len(), 2);
        assert_ne!(layout.focused(), second);
        assert!(layout.contains(layout.focused()));
    }

    #[test]
    fn only_keeps_the_focused_pane() {
        let mut layout = Layout::new();
        let second = layout.split(Axis::Vertical);
        let third = layout.split(Axis::Horizontal);
        layout.focus(second);
        let mut gone = layout.only();
        gone.sort();
        assert_eq!(gone, [0, third]);
        assert_eq!(layout.panes(), [second]);
        assert!(!layout.is_split());
    }

    #[test]
    fn pane_ids_arent_reused() {
        let mut layout = Layout::new();
        let a = layout.split(Axis::Vertical);
        layout.close(a);
        let b2 = layout.split(Axis::Vertical);
        assert_ne!(a, b2);
    }

    #[test]
    fn resize_grows_the_focused_pane_and_shrinks_its_neighbour() {
        let mut layout = Layout::new();
        layout.split(Axis::Vertical);
        assert!(layout.resize(Axis::Vertical, 0.1));
        let widths: Vec<i32> = layout
            .bounds(1000, 600, 0)
            .iter()
            .map(|(_, b)| b.width)
            .collect();
        assert_eq!(widths, [400, 600], "the focused right pane is wider");
        assert!(
            !layout.resize(Axis::Horizontal, 0.1),
            "no split runs that way"
        );
        assert!(layout.resize(Axis::Vertical, 5.0));
        let widths: Vec<i32> = layout
            .bounds(1000, 600, 0)
            .iter()
            .map(|(_, b)| b.width)
            .collect();
        assert_eq!(widths, [100, 900], "the other pane keeps a little room");
    }

    #[test]
    fn resize_finds_the_split_that_runs_the_right_way() {
        let mut layout = Layout::new();
        layout.split(Axis::Vertical);
        layout.split(Axis::Horizontal);
        // Taller within the right column, then wider against the left pane.
        assert!(layout.resize(Axis::Horizontal, 0.2));
        assert!(layout.resize(Axis::Vertical, -0.1));
        assert_eq!(
            layout.bounds(1000, 1000, 0),
            [
                (0, b(0, 0, 600, 1000)),
                (1, b(600, 0, 400, 300)),
                (2, b(600, 300, 400, 700)),
            ]
        );
        assert!(!Layout::new().resize(Axis::Vertical, 0.1));
    }

    #[test]
    fn a_layout_round_trips_through_a_tree() {
        let mut layout = Layout::new();
        let right = layout.split(Axis::Vertical);
        let below = layout.split(Axis::Horizontal);
        layout.resize(Axis::Vertical, 0.1);
        let tabs = |p: PaneId| {
            Some(match p {
                0 => "a",
                p if p == right => "b",
                p if p == below => "c",
                _ => unreachable!(),
            })
        };
        let tree = layout.tree(tabs).unwrap();
        let (restored, panes) = Layout::from_tree(tree, |t| *t == "c");
        assert_eq!(restored.bounds(1000, 1000, 2), layout.bounds(1000, 1000, 2));
        let focused = panes
            .iter()
            .find(|(p, _)| *p == restored.focused())
            .unwrap();
        assert_eq!(focused.1, "c");
        assert_eq!(
            panes.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn panes_without_a_tab_are_left_out_of_the_tree() {
        let mut layout = Layout::new();
        let right = layout.split(Axis::Vertical);
        let below = layout.split(Axis::Horizontal);
        let tree = layout.tree(|p| (p != below).then_some(p)).unwrap();
        assert_eq!(
            tree,
            Tree::Split {
                axis: Axis::Vertical,
                children: vec![(Tree::Pane(0), 0.5), (Tree::Pane(right), 0.5)],
            }
        );
        assert_eq!(
            layout.tree(|p| (p == 0).then_some(p)),
            None,
            "one pane isn't a split"
        );
        assert_eq!(Layout::new().tree(Some), None);
    }

    #[test]
    fn saved_shares_that_dont_add_up_are_fixed() {
        let tree = Tree::Split {
            axis: Axis::Vertical,
            children: vec![(Tree::Pane('a'), 3.0), (Tree::Pane('b'), 1.0)],
        };
        let (layout, _) = Layout::from_tree(tree, |_| false);
        let widths: Vec<i32> = layout
            .bounds(800, 600, 0)
            .iter()
            .map(|(_, b)| b.width)
            .collect();
        assert_eq!(widths, [600, 200]);
        assert_eq!(layout.focused(), 0);
    }

    #[test]
    fn borders_sit_between_panes_and_move_by_pixels() {
        let mut layout = Layout::new();
        layout.split(Axis::Vertical);
        layout.split(Axis::Horizontal);
        let borders = layout.borders(1000, 800, 6);
        assert_eq!(
            borders,
            [
                Border {
                    id: ":0".into(),
                    axis: Axis::Vertical,
                    bounds: b(497, 0, 6, 800),
                    extent: 1000,
                },
                Border {
                    id: "1:0".into(),
                    axis: Axis::Horizontal,
                    bounds: b(500, 397, 500, 6),
                    extent: 800,
                },
            ]
        );
        assert!(layout.move_border(":0", 100, 1000));
        assert!(layout.move_border("1:0", -200, 800));
        assert_eq!(
            layout.bounds(1000, 800, 0),
            [
                (0, b(0, 0, 600, 800)),
                (1, b(600, 0, 400, 200)),
                (2, b(600, 200, 400, 600)),
            ]
        );
        assert!(
            layout.move_border(":0", 5000, 1000),
            "it stops short of the edge"
        );
        assert_eq!(layout.bounds(1000, 800, 0)[0].1.width, 900);
        assert!(!layout.move_border("1:1", 10, 800));
        assert!(!layout.move_border("bad", 10, 800));
        assert!(Layout::new().borders(1000, 800, 6).is_empty());
    }

    #[test]
    fn directions_parse() {
        assert_eq!(Direction::parse("left"), Some(Direction::Left));
        assert_eq!(Direction::parse("down"), Some(Direction::Down));
        assert_eq!(Direction::parse("sideways"), None);
    }
}
