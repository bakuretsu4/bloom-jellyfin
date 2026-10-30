//! Fitting the picture into a box inside the window, as mpv's margins, zoom and pan.

#[derive(Debug, PartialEq)]
pub struct Crop {
    /// Left, top, right, bottom, as mpv takes them: within the window.
    pub margins: [f64; 4],
    /// log2 of the scale, as mpv's video-zoom.
    pub zoom: f64,
    /// Fractions of the scaled picture's size, as mpv's video-pan.
    pub pan_x: f64,
    pub pan_y: f64,
}

/// mpv's margins stop at the window's edge, so a box partly outside the window is drawn by
/// fitting the picture to the part inside, then zooming and panning it back to the size and
/// place of the whole box; the rest falls outside the window. A box wholly outside shrinks the
/// picture to nothing, so it can't show anywhere the page isn't painted.
///
/// `sides` are the fractions of the window left free on each side (a negative side is a box
/// running past the edge); `window_aspect` is the window's width over its height.
pub fn crop(sides: [f64; 4], window_aspect: f64, video_aspect: Option<f64>) -> Crop {
    let [l, t, r, b] = sides.map(|v| if v.is_finite() { v } else { 0.0 });
    let margins = [l, t, r, b].map(|v| v.clamp(0.0, 0.95));
    let plain = Crop { margins, zoom: 0.0, pan_x: 0.0, pan_y: 0.0 };
    if margins == [l, t, r, b] {
        return plain;
    }
    if 1.0 - l.max(0.0) - r.max(0.0) <= 0.0 || 1.0 - t.max(0.0) - b.max(0.0) <= 0.0 {
        return Crop { zoom: -20.0, ..plain };
    }
    let Some(video) = video_aspect.filter(|a| a.is_finite() && *a > 0.0) else { return plain };
    let w = if window_aspect.is_finite() && window_aspect > 0.0 { window_aspect } else { 16.0 / 9.0 };
    // Measured in a window `w` wide and 1 high.
    let (box_w, box_h) = ((1.0 - l - r) * w, 1.0 - t - b);
    let (area_w, area_h) = ((1.0 - margins[0] - margins[2]) * w, 1.0 - margins[1] - margins[3]);
    if box_w <= 0.0 || box_h <= 0.0 || area_w <= 0.0 || area_h <= 0.0 {
        return plain;
    }
    // The picture's width when fitted into an area, keeping its shape.
    let fitted = |aw: f64, ah: f64| if aw / ah > video { ah * video } else { aw };
    let (picture_w, area_picture_w) = (fitted(box_w, box_h), fitted(area_w, area_h));
    let picture_h = picture_w / video;
    let centre = |start: f64, size: f64| start + size / 2.0;
    Crop {
        margins,
        zoom: (picture_w / area_picture_w).log2(),
        pan_x: ((centre(l * w, box_w) - centre(margins[0] * w, area_w)) / picture_w).clamp(-3.0, 3.0),
        pan_y: ((centre(t, box_h) - centre(margins[1], area_h)) / picture_h).clamp(-3.0, 3.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: f64 = 16.0 / 9.0;

    #[test]
    fn a_box_inside_the_window_is_only_margins() {
        let view = crop([0.1, 0.1, 0.1, 0.2], WIDE, Some(WIDE));
        assert_eq!(view, Crop { margins: [0.1, 0.1, 0.1, 0.2], zoom: 0.0, pan_x: 0.0, pan_y: 0.0 });
    }

    #[test]
    fn a_box_scrolled_past_the_top_is_zoomed_and_panned_back_into_place() {
        // A full-width 16:9 box, a quarter of the window's height above the top edge.
        let view = crop([0.0, -0.25, 0.0, 0.25], WIDE, Some(WIDE));
        assert_eq!(view.margins, [0.0, 0.0, 0.0, 0.25]);
        // Fitted to the visible 0.75, the picture is 0.75 high; the box is 1.0 high.
        assert!((2f64.powf(view.zoom) - 1.0 / 0.75).abs() < 1e-9, "zoom {}", view.zoom);
        // Its centre moves from 0.375 (the visible part's) to 0.25 (the box's): an eighth of its height.
        assert!((view.pan_y + 0.125).abs() < 1e-9, "pan {}", view.pan_y);
        assert_eq!(view.pan_x, 0.0);
    }

    #[test]
    fn a_box_wholly_above_the_window_hides_the_picture() {
        assert_eq!(crop([0.0, -1.5, 0.0, 1.2], WIDE, Some(WIDE)).zoom, -20.0);
    }

    #[test]
    fn without_a_picture_yet_there_is_nothing_to_zoom() {
        assert_eq!(crop([0.0, -0.25, 0.0, 0.25], WIDE, None).zoom, 0.0);
    }
}
