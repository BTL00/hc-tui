//! The Herdr app's shared layout document (`herdr-app` in each machine's core):
//! spaces → tabs → a tree of `row` / `column` / `stack` containers and `block`s
//! showing terminals. Edited as raw JSON so fields this build doesn't know
//! survive our writes, as the Herdr app does itself.
//!
//! Schema per herdr-app `src/mirror.rs` and `src/layout.rs` (Apache-2.0).

use ratatui::layout::Rect;
use serde_json::{Value, json};
use std::hash::{BuildHasher, Hasher};

pub const DOC: &str = "herdr-app";

/// A fresh id like the app's: a prefix and 32 hex digits.
pub fn new_id(prefix: &str) -> String {
    let mut out = String::from(prefix);
    for _ in 0..2 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

pub fn empty_doc() -> Value {
    json!({"spaces": []})
}

pub fn block(terminal: &str) -> Value {
    json!({"id": new_id("p"), "type": "block", "content": {"terminal": terminal}})
}

pub fn tab(name: &str, terminal: &str) -> Value {
    let id = new_id("tb");
    json!({"id": id, "name": name, "root": {"id": id, "type": "row", "children": [block(terminal)]}})
}

pub fn space(name: &str, folder: &str, first_tab: Value) -> Value {
    json!({"id": new_id("sp"), "name": name, "folder": folder, "tabs": [first_tab]})
}

pub fn spaces_mut(doc: &mut Value) -> &mut Vec<Value> {
    if !doc.is_object() {
        *doc = empty_doc();
    }
    if !doc["spaces"].is_array() {
        doc["spaces"] = json!([]);
    }
    doc["spaces"].as_array_mut().expect("spaces is an array")
}

pub fn find_space<'a>(doc: &'a mut Value, id: &str) -> Option<&'a mut Value> {
    spaces_mut(doc)
        .iter_mut()
        .find(|s| s["id"].as_str() == Some(id))
}

pub fn find_tab<'a>(doc: &'a mut Value, space: &str, tab: &str) -> Option<&'a mut Value> {
    find_space(doc, space)?["tabs"]
        .as_array_mut()?
        .iter_mut()
        .find(|t| t["id"].as_str() == Some(tab))
}

fn kind(node: &Value) -> &str {
    node["type"].as_str().unwrap_or("")
}

fn block_terminal(node: &Value) -> Option<&str> {
    (kind(node) == "block")
        .then(|| node["content"]["terminal"].as_str())
        .flatten()
}

/// Terminal ids in a tree, in reading order.
pub fn terminals(node: &Value, out: &mut Vec<String>) {
    if let Some(t) = block_terminal(node) {
        out.push(t.to_string());
    }
    if let Some(children) = node["children"].as_array() {
        for c in children {
            terminals(c, out);
        }
    }
}

/// Splits the block showing `target`, putting `new` after it along `axis`
/// (`row`: to the right, `column`: below).
pub fn split(root: &mut Value, target: &str, new: &str, axis: &str) -> bool {
    if block_terminal(root) == Some(target) {
        let old = root.take();
        *root = json!({"id": new_id("s"), "type": axis, "children": [old, block(new)]});
        return true;
    }
    split_in(root, target, new, axis)
}

fn split_in(node: &mut Value, target: &str, new: &str, axis: &str) -> bool {
    let same_axis = kind(node) == axis;
    let Some(children) = node["children"].as_array_mut() else {
        return false;
    };
    for i in 0..children.len() {
        if block_terminal(&children[i]) == Some(target) {
            if same_axis {
                let mut b = block(new);
                if let Some(w) = children[i]["size"]["weight"].as_f64() {
                    // Halve the old pane's share between it and the new one.
                    children[i]["size"] = json!({"weight": w / 2.0});
                    b["size"] = json!({"weight": w / 2.0});
                }
                children.insert(i + 1, b);
            } else {
                let mut old = children[i].take();
                let size = old.as_object_mut().and_then(|o| o.remove("size"));
                let mut wrap =
                    json!({"id": new_id("s"), "type": axis, "children": [old, block(new)]});
                if let Some(size) = size {
                    wrap["size"] = size;
                }
                children[i] = wrap;
            }
            return true;
        }
        if split_in(&mut children[i], target, new, axis) {
            return true;
        }
    }
    false
}

/// Removes the block showing `target`; containers left with one child give
/// way to it, empty ones disappear. Returns whether it was found.
pub fn remove(root: &mut Value, target: &str) -> bool {
    let found = remove_in(root, target);
    if found {
        normalize(root, true);
    }
    found
}

fn remove_in(node: &mut Value, target: &str) -> bool {
    let Some(children) = node["children"].as_array_mut() else {
        return false;
    };
    if let Some(i) = children
        .iter()
        .position(|c| block_terminal(c) == Some(target))
    {
        children.remove(i);
        return true;
    }
    children.iter_mut().any(|c| remove_in(c, target))
}

fn normalize(node: &mut Value, is_root: bool) {
    if let Some(children) = node["children"].as_array_mut() {
        for c in children.iter_mut() {
            normalize(c, false);
        }
        children.retain(|c| {
            !(c["children"].is_array() && c["children"].as_array().is_some_and(|a| a.is_empty()))
        });
        if !is_root && children.len() == 1 && kind(node) != "stack" {
            let size = node.get("size").cloned();
            let mut only = node["children"].as_array_mut().unwrap().remove(0);
            if let Some(size) = size {
                only["size"] = size;
            }
            *node = only;
        }
    }
}

/// Swaps which terminals two blocks show.
pub fn swap(root: &mut Value, a: &str, b: &str) {
    if let Some(t) = block_terminal(root).map(str::to_string) {
        let other = if t == a {
            Some(b)
        } else if t == b {
            Some(a)
        } else {
            None
        };
        if let Some(o) = other {
            root["content"]["terminal"] = Value::String(o.to_string());
        }
    }
    if let Some(children) = root["children"].as_array_mut() {
        for c in children {
            swap(c, a, b);
        }
    }
}

/// Grows (or shrinks, negative) the pane holding `target` along `axis` by
/// moving weight from/to its next sibling in the nearest such container.
pub fn resize(root: &mut Value, target: &str, axis: &str, delta: f64) -> bool {
    let mut path = Vec::new();
    if !path_to(root, target, &mut path) {
        return false;
    }
    // Deepest container of that axis on the path.
    for depth in (0..path.len()).rev() {
        let node = at_path(root, &path[..depth]);
        if kind(node) != axis {
            continue;
        }
        let i = path[depth];
        let children = node["children"].as_array_mut().unwrap();
        let n = children.len();
        if n < 2 {
            return false;
        }
        let j = if i + 1 < n { i + 1 } else { i - 1 };
        let sign = if j > i { 1.0 } else { -1.0 };
        let weight = |c: &Value| c["size"]["weight"].as_f64().unwrap_or(1.0);
        for c in children.iter_mut() {
            if c["size"]["px"].is_number() {
                return false; // fixed-size panes stay as the layout made them
            }
            let w = weight(c);
            c["size"] = json!({"weight": w});
        }
        let total: f64 = children.iter().map(weight).sum();
        let step = delta * sign * total / 10.0;
        let (wi, wj) = (weight(&children[i]), weight(&children[j]));
        let floor = total * 0.08;
        if wi + step < floor || wj - step < floor {
            return false;
        }
        children[i]["size"] = json!({"weight": wi + step});
        children[j]["size"] = json!({"weight": wj - step});
        return true;
    }
    false
}

fn path_to(node: &Value, target: &str, path: &mut Vec<usize>) -> bool {
    if block_terminal(node) == Some(target) {
        return true;
    }
    if let Some(children) = node["children"].as_array() {
        for (i, c) in children.iter().enumerate() {
            path.push(i);
            if path_to(c, target, path) {
                return true;
            }
            path.pop();
        }
    }
    false
}

fn at_path<'a>(mut node: &'a mut Value, path: &[usize]) -> &'a mut Value {
    for &i in path {
        node = &mut node["children"][i];
    }
    node
}

/// Where each visible terminal goes inside `area`, plus divider lines.
pub struct Placed {
    pub panes: Vec<(String, Rect)>,
    pub dividers: Vec<(Rect, bool)>,
}

pub fn place(root: &Value, area: Rect, zoomed: Option<&str>) -> Placed {
    let mut out = Placed {
        panes: Vec::new(),
        dividers: Vec::new(),
    };
    if let Some(z) = zoomed {
        let mut all = Vec::new();
        terminals(root, &mut all);
        if all.iter().any(|t| t == z) {
            out.panes.push((z.to_string(), area));
            return out;
        }
    }
    place_in(root, area, &mut out);
    out
}

fn place_in(node: &Value, area: Rect, out: &mut Placed) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    match kind(node) {
        "block" => {
            if let Some(t) = block_terminal(node) {
                out.panes.push((t.to_string(), area));
            }
        }
        "stack" => {
            let children: Vec<&Value> = visible(node);
            let active = node["active"].as_str();
            let shown = children
                .iter()
                .find(|c| active.is_some() && c["id"].as_str() == active)
                .or(children.first());
            if let Some(c) = shown {
                place_in(c, area, out);
            }
        }
        k @ ("row" | "column") => {
            let horizontal = k == "row";
            let children = visible(node);
            if children.is_empty() {
                return;
            }
            let total = if horizontal { area.width } else { area.height };
            let gaps = children.len() as u16 - 1;
            let avail = total.saturating_sub(gaps);
            // Fixed px sizes (assume ~8px per column, ~16px per row), weights share the rest.
            let cell_px = if horizontal { 8.0 } else { 16.0 };
            let fixed: Vec<Option<u16>> = children
                .iter()
                .map(|c| {
                    c["size"]["px"]
                        .as_f64()
                        .map(|px| (px / cell_px).round().max(1.0) as u16)
                })
                .collect();
            let fixed_sum: u16 = fixed.iter().flatten().sum::<u16>().min(avail);
            let weights: Vec<f64> = children
                .iter()
                .zip(&fixed)
                .map(|(c, f)| {
                    if f.is_some() {
                        0.0
                    } else {
                        c["size"]["weight"].as_f64().unwrap_or(1.0).max(0.01)
                    }
                })
                .collect();
            let wsum: f64 = weights.iter().sum::<f64>().max(0.01);
            let flexible = avail - fixed_sum;
            let mut sizes: Vec<u16> = fixed
                .iter()
                .zip(&weights)
                .map(|(f, w)| f.unwrap_or_else(|| ((flexible as f64) * w / wsum).floor() as u16))
                .collect();
            // Hand out what rounding left over to the last flexible child.
            let used: u16 = sizes.iter().sum();
            if let Some(i) = fixed.iter().rposition(|f| f.is_none()) {
                sizes[i] += avail.saturating_sub(used);
            }
            let mut pos = if horizontal { area.x } else { area.y };
            for (i, (c, size)) in children.iter().zip(&sizes).enumerate() {
                let r = if horizontal {
                    Rect {
                        x: pos,
                        y: area.y,
                        width: *size,
                        height: area.height,
                    }
                } else {
                    Rect {
                        x: area.x,
                        y: pos,
                        width: area.width,
                        height: *size,
                    }
                };
                place_in(c, r, out);
                pos += size;
                if i + 1 < children.len() {
                    let d = if horizontal {
                        Rect {
                            x: pos,
                            y: area.y,
                            width: 1,
                            height: area.height,
                        }
                    } else {
                        Rect {
                            x: area.x,
                            y: pos,
                            width: area.width,
                            height: 1,
                        }
                    };
                    out.dividers.push((d, horizontal));
                    pos += 1;
                }
            }
        }
        _ => {}
    }
}

fn visible(node: &Value) -> Vec<&Value> {
    node["children"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|c| c["hidden"].as_bool() != Some(true))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(root: &Value) -> Vec<String> {
        let mut v = Vec::new();
        terminals(root, &mut v);
        v
    }

    #[test]
    fn split_remove_round_trip() {
        let mut t = tab("main", "t1");
        let root = &mut t["root"];
        assert!(split(root, "t1", "t2", "row"));
        assert_eq!(ids(root), ["t1", "t2"]);
        assert_eq!(
            root["children"].as_array().unwrap().len(),
            2,
            "same axis: a sibling"
        );
        assert!(split(root, "t2", "t3", "column"));
        assert_eq!(ids(root), ["t1", "t2", "t3"]);
        assert_eq!(root["children"][1]["type"], "column");
        assert!(remove(root, "t2"));
        assert_eq!(ids(root), ["t1", "t3"]);
        assert_eq!(
            root["children"][1]["type"], "block",
            "a one-child column gives way"
        );
        assert!(remove(root, "t1"));
        assert!(remove(root, "t3"));
        assert!(ids(root).is_empty());
    }

    #[test]
    fn unknown_fields_survive_edits() {
        let mut t = tab("main", "t1");
        t["root"]["background"] = json!("#112233");
        t["mystery"] = json!(42);
        split(&mut t["root"], "t1", "t2", "column");
        assert_eq!(t["root"]["background"], "#112233");
        assert_eq!(t["mystery"], 42);
    }

    #[test]
    fn placement_shares_space_by_weight() {
        let mut t = tab("main", "t1");
        split(&mut t["root"], "t1", "t2", "row");
        let p = place(&t["root"], Rect::new(0, 0, 81, 20), None);
        assert_eq!(p.panes.len(), 2);
        assert_eq!(p.panes[0].1.width + p.panes[1].1.width + 1, 81);
        assert_eq!(p.dividers.len(), 1);
        assert!(resize(&mut t["root"], "t1", "row", 1.0));
        let p = place(&t["root"], Rect::new(0, 0, 81, 20), None);
        assert!(p.panes[0].1.width > p.panes[1].1.width);
        let z = place(&t["root"], Rect::new(0, 0, 81, 20), Some("t2"));
        assert_eq!(z.panes, vec![("t2".to_string(), Rect::new(0, 0, 81, 20))]);
    }

    #[test]
    fn swap_exchanges_terminals() {
        let mut t = tab("main", "t1");
        split(&mut t["root"], "t1", "t2", "row");
        swap(&mut t["root"], "t1", "t2");
        assert_eq!(ids(&t["root"]), ["t2", "t1"]);
    }
}
