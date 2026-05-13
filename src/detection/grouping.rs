use crate::detection::obico::Detection;
use crate::printer::state::DetectionPoint;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectionGroup {
    pub representative: DetectionPoint,
    pub count: usize,
    pub ts_first: u64,
    pub ts_last: u64,
    pub score_max: f64,
    pub score_min: f64,
    pub snapshots: Vec<GroupSnapshot>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GroupSnapshot {
    pub ts: u64,
    pub score: f64,
    pub filename: String,
    pub boxes: Vec<Detection>,
}

/// group by time and iou
pub fn group_detection_points(
    points: &[DetectionPoint],
    window_secs: u64,
    iou_threshold: f64,
) -> Vec<DetectionGroup> {
    let mut sorted: Vec<DetectionPoint> = points.to_vec();
    sorted.sort_by_key(|p| p.ts);

    struct OpenGroup {
        members: Vec<DetectionPoint>,
        representative: DetectionPoint,
        ts_last: u64,
    }

    let mut open: Vec<OpenGroup> = Vec::new();
    let mut done: Vec<DetectionGroup> = Vec::new();

    for pt in sorted {
        // expire groups that aged out
        let mut active = Vec::new();
        for g in open.drain(..) {
            if pt.ts.saturating_sub(g.ts_last) <= window_secs {
                active.push(g);
            } else {
                done.push(finalize(g.members));
            }
        }
        open = active;

        // find compatible open group
        let mut matched = None;
        for (i, g) in open.iter().enumerate() {
            if pt.print_filename == g.representative.print_filename
                && boxes_compatible(&pt, &g.representative, iou_threshold)
            {
                matched = Some(i);
                break;
            }
        }

        if let Some(i) = matched {
            let g = &mut open[i];
            if pt.score > g.representative.score {
                g.representative = pt.clone();
            }
            g.ts_last = pt.ts;
            g.members.push(pt);
        } else {
            open.push(OpenGroup {
                representative: pt.clone(),
                ts_last: pt.ts,
                members: vec![pt],
            });
        }
    }

    for g in open {
        done.push(finalize(g.members));
    }

    done.sort_by(|a, b| b.ts_last.cmp(&a.ts_last));
    done
}

fn finalize(members: Vec<DetectionPoint>) -> DetectionGroup {
    let representative = members
        .iter()
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap()
        .clone();

    let score_max = members.iter().map(|p| p.score).fold(0.0_f64, f64::max);
    let score_min = members.iter().map(|p| p.score).fold(f64::INFINITY, f64::min);
    let ts_first = members.iter().map(|p| p.ts).min().unwrap_or(0);
    let ts_last = members.iter().map(|p| p.ts).max().unwrap_or(0);

    let mut snapshots: Vec<GroupSnapshot> = members
        .iter()
        .filter_map(|p| {
            p.snapshot.as_ref().map(|f| GroupSnapshot {
                ts: p.ts,
                score: p.score,
                filename: f.clone(),
                boxes: p.boxes.clone(),
            })
        })
        .collect();
    snapshots.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    DetectionGroup {
        representative,
        count: members.len(),
        ts_first,
        ts_last,
        score_max,
        score_min,
        snapshots,
    }
}

fn boxes_compatible(a: &DetectionPoint, b: &DetectionPoint, threshold: f64) -> bool {
    let a_empty = a.boxes.is_empty();
    let b_empty = b.boxes.is_empty();
    match (a_empty, b_empty) {
        (true, true) => true,
        (false, false) => iou(dominant(&a.boxes), dominant(&b.boxes)) >= threshold,
        _ => false,
    }
}

fn dominant(boxes: &[Detection]) -> &Detection {
    boxes
        .iter()
        .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap()
}

fn iou(a: &Detection, b: &Detection) -> f64 {
    let ix1 = a.x1.max(b.x1);
    let iy1 = a.y1.max(b.y1);
    let ix2 = a.x2.min(b.x2);
    let iy2 = a.y2.min(b.y2);
    let inter = (ix2 - ix1).max(0.0) * (iy2 - iy1).max(0.0);
    if inter == 0.0 {
        return 0.0;
    }
    let area_a = (a.x2 - a.x1).max(0.0) * (a.y2 - a.y1).max(0.0);
    let area_b = (b.x2 - b.x1).max(0.0) * (b.y2 - b.y1).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 {
        return 0.0;
    }
    inter / union
}
