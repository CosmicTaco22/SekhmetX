// Ported from IsiX:
// - Modules/IsiXGeometry/Sources/BezierCore.swift
// - Modules/IsiXGeometry/Sources/BezierCore+Editing.swift
// - Modules/IsiXGeometry/Sources/BezierCore+Measuring.swift
// - Modules/IsiXGeometry/Sources/BezierCore+Nodes.swift
// - Modules/IsiXGeometry/Sources/BezierCore+Sampling.swift

use super::line3::Line3;
use super::plane3::Plane3;
use super::transform3d::Transform3D;
use super::vector3::{adding_product, Vector3};

/// What a segment of a path does. Raw values match IsiX `BezierSegmentKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BezierSegmentKind {
    MoveTo = 0,
    LineTo = 1,
    CurveTo = 2,
    Close = 3,
}

/// One segment of a 3D Bézier path. Controls are only meaningful for `CurveTo`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BezierSegment {
    pub kind: BezierSegmentKind,
    pub control1: Vector3,
    pub control2: Vector3,
    pub endpoint: Vector3,
}

impl BezierSegment {
    pub const fn new(
        kind: BezierSegmentKind,
        control1: Vector3,
        control2: Vector3,
        endpoint: Vector3,
    ) -> Self {
        Self {
            kind,
            control1,
            control2,
            endpoint,
        }
    }

    pub const fn linear(kind: BezierSegmentKind, endpoint: Vector3) -> Self {
        Self::new(kind, Vector3::ZERO, Vector3::ZERO, endpoint)
    }

    pub fn matches(&self, other: &Self) -> bool {
        if self.kind != other.kind || self.endpoint != other.endpoint {
            return false;
        }
        self.kind != BezierSegmentKind::CurveTo
            || (self.control1 == other.control1 && self.control2 == other.control2)
    }

    fn approximate_length(&self, start: Vector3) -> f64 {
        match self.kind {
            BezierSegmentKind::LineTo | BezierSegmentKind::Close => self.endpoint.distance(start),
            BezierSegmentKind::CurveTo => {
                start.distance(self.control1)
                    + self.control1.distance(self.control2)
                    + self.control2.distance(self.endpoint)
            }
            BezierSegmentKind::MoveTo => 0.0,
        }
    }

    fn flatness(&self, start: Vector3) -> f64 {
        if self.kind != BezierSegmentKind::CurveTo {
            return 0.0;
        }
        let line = self.endpoint - start;
        let vec_to_c1 = self.control1 - start;
        let vec_to_c2 = self.control2 - self.endpoint;
        let line_len = line.length();

        let c1_proj = line.dot_product(vec_to_c1) / line_len;
        let end_flat1 = -c1_proj;
        let flat1 = (vec_to_c1 - line * (c1_proj / line_len)).length();

        let c2_proj = line.dot_product(vec_to_c2) / line_len;
        let end_flat2 = c2_proj;
        let flat2 = (vec_to_c2 - line * (c2_proj / line_len)).length();

        let mut max_flat = flat1;
        if flat2 > max_flat {
            max_flat = flat2;
        }
        if end_flat1 > max_flat {
            max_flat = end_flat1;
        }
        if end_flat2 > max_flat {
            max_flat = end_flat2;
        }
        max_flat
    }

    fn divided(&self, start: Vector3) -> (Self, Self) {
        debug_assert!(self.kind != BezierSegmentKind::MoveTo);
        match self.kind {
            BezierSegmentKind::CurveTo => {
                let q0 = (start + self.control1) * 0.5;
                let q1 = (self.control1 + self.control2) * 0.5;
                let q2 = (self.control2 + self.endpoint) * 0.5;
                let r0 = (q0 + q1) * 0.5;
                let r1 = (q1 + q2) * 0.5;
                let b = (r0 + r1) * 0.5;
                (
                    Self::new(BezierSegmentKind::CurveTo, q0, r0, b),
                    Self::new(BezierSegmentKind::CurveTo, r1, q2, self.endpoint),
                )
            }
            BezierSegmentKind::Close => (
                Self::linear(BezierSegmentKind::LineTo, (start + self.endpoint) * 0.5),
                Self::linear(BezierSegmentKind::Close, self.endpoint),
            ),
            _ => (
                Self::linear(self.kind, (start + self.endpoint) * 0.5),
                Self::linear(self.kind, self.endpoint),
            ),
        }
    }
}

/// How the ends of a curve through nodes are shaped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BezierNodeStyle {
    /// End segments point out (CPR paths).
    OpenEnds = 0,
    /// End segments point to each other (open ROIs).
    EndsMeet = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BezierClosestPoint {
    pub relative_position: f64,
    pub vector: Vector3,
    pub distance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BezierPlaneIntersection {
    pub vector: Vector3,
    pub relative_position: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BezierSamples {
    pub vectors: Vec<Vector3>,
    pub tangents: Vec<Vector3>,
    pub normals: Vec<Vector3>,
    pub relative_positions: Vec<f64>,
    pub count: usize,
}

struct EndpointCursor<'a> {
    segments: &'a [BezierSegment],
    index: usize,
}

impl<'a> EndpointCursor<'a> {
    fn new(core: &'a BezierCore) -> Self {
        Self {
            segments: &core.segments,
            index: 0,
        }
    }

    fn is_at_end(&self) -> bool {
        self.index >= self.segments.len()
    }

    fn next(&mut self) -> BezierSegment {
        if self.is_at_end() {
            return BezierSegment::linear(BezierSegmentKind::MoveTo, Vector3::ZERO);
        }
        let seg = self.segments[self.index];
        self.index += 1;
        seg
    }
}

/// A path of segments in 3D space, ported from IsiX `BezierCore`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BezierCore {
    pub segments: Vec<BezierSegment>,
}

impl BezierCore {
    pub const DEFAULT_FLATNESS: f64 = 0.1;
    pub const DEFAULT_SUBDIVIDE_SEGMENT_LENGTH: f64 = 3.0;

    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.segments.len()
    }

    pub fn add(
        &mut self,
        kind: BezierSegmentKind,
        control1: Vector3,
        control2: Vector3,
        endpoint: Vector3,
    ) {
        debug_assert!(!self.segments.is_empty() || kind == BezierSegmentKind::MoveTo);
        debug_assert!(
            self.segments.last().map(|s| s.kind) != Some(BezierSegmentKind::Close)
                || kind == BezierSegmentKind::MoveTo
        );
        match kind {
            BezierSegmentKind::MoveTo | BezierSegmentKind::LineTo => {
                self.segments.push(BezierSegment::linear(kind, endpoint));
            }
            BezierSegmentKind::CurveTo => {
                self.segments
                    .push(BezierSegment::new(kind, control1, control2, endpoint));
            }
            BezierSegmentKind::Close => {
                let last_move = self.last_move_to();
                self.segments.push(BezierSegment::linear(kind, last_move));
            }
        }
    }

    pub fn add_segment(&mut self, segment: BezierSegment) {
        self.add(
            segment.kind,
            segment.control1,
            segment.control2,
            segment.endpoint,
        );
    }

    fn last_move_to(&self) -> Vector3 {
        self.segments
            .iter()
            .rev()
            .find(|s| s.kind == BezierSegmentKind::MoveTo)
            .map(|s| s.endpoint)
            .unwrap_or(Vector3::ZERO)
    }

    pub fn set_vectors(
        &mut self,
        index: usize,
        control1: Vector3,
        control2: Vector3,
        endpoint: Vector3,
    ) {
        assert!(index < self.segments.len());
        match self.segments[index].kind {
            BezierSegmentKind::MoveTo => {
                self.segments[index].endpoint = endpoint;
                for following in (index + 1)..self.segments.len() {
                    if self.segments[following].kind == BezierSegmentKind::Close {
                        self.segments[following].endpoint = endpoint;
                        break;
                    } else if self.segments[following].kind == BezierSegmentKind::MoveTo {
                        break;
                    }
                }
            }
            BezierSegmentKind::LineTo => {
                self.segments[index].endpoint = endpoint;
            }
            BezierSegmentKind::CurveTo => {
                self.segments[index].control1 = control1;
                self.segments[index].control2 = control2;
                self.segments[index].endpoint = endpoint;
            }
            BezierSegmentKind::Close => {}
        }
    }

    pub fn flatten(&mut self, flatness: f64) {
        if self.segments.len() < 2 {
            return;
        }
        let flatness = if flatness == 0.0 {
            Self::DEFAULT_FLATNESS
        } else {
            flatness
        };
        self.segments = self.splitting(|segment, start| {
            if segment.flatness(start) > flatness {
                return None;
            }
            if segment.kind != BezierSegmentKind::CurveTo {
                return Some(segment);
            }
            Some(BezierSegment::linear(
                BezierSegmentKind::LineTo,
                segment.endpoint,
            ))
        });
    }

    pub fn subdivide(&mut self, max_segment_length: f64) {
        if self.segments.len() < 2 {
            return;
        }
        let max_len = if max_segment_length == 0.0 {
            Self::DEFAULT_SUBDIVIDE_SEGMENT_LENGTH
        } else {
            max_segment_length
        };
        self.segments = self.splitting(|segment, start| {
            if segment.approximate_length(start) > max_len {
                None
            } else {
                Some(segment)
            }
        });
    }

    fn splitting<F>(&self, mut keep: F) -> Vec<BezierSegment>
    where
        F: FnMut(BezierSegment, Vector3) -> Option<BezierSegment>,
    {
        let mut result = Vec::with_capacity(self.segments.len() * 2);
        result.push(self.segments[0]);

        fn place<F>(
            segment: BezierSegment,
            result: &mut Vec<BezierSegment>,
            keep: &mut F,
        ) where
            F: FnMut(BezierSegment, Vector3) -> Option<BezierSegment>,
        {
            let start = result[result.len() - 1].endpoint;
            if let Some(kept) = keep(segment, start) {
                result.push(kept);
            } else {
                let (first, second) = segment.divided(start);
                place(first, result, keep);
                place(second, result, keep);
            }
        }

        for &segment in &self.segments[1..] {
            place(segment, &mut result, &mut keep);
        }
        result
    }

    pub fn apply(&mut self, transform: Transform3D) {
        for seg in &mut self.segments {
            seg.endpoint = seg.endpoint.applying(transform);
            if seg.kind == BezierSegmentKind::CurveTo {
                seg.control1 = seg.control1.applying(transform);
                seg.control2 = seg.control2.applying(transform);
            }
        }
    }

    pub fn append(&mut self, core: &Self, connecting_paths: bool) {
        let mut appended = &core.segments[..];
        if !appended.is_empty() && connecting_paths {
            appended = &appended[1..];
            if self.segments.last().map(|s| s.kind) == Some(BezierSegmentKind::Close) {
                self.segments.pop();
            }
        }
        for &segment in appended {
            self.add_segment(segment);
        }
    }

    pub fn has_curve(&self) -> bool {
        self.segments
            .iter()
            .skip(1)
            .any(|s| s.kind == BezierSegmentKind::CurveTo)
    }

    pub fn subpath_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|s| s.kind == BezierSegmentKind::MoveTo)
            .count()
    }

    pub fn length(&self) -> f64 {
        let Some(first) = self.segments.first() else {
            return 0.0;
        };
        if self.has_curve() {
            return self.flattened_copy(Self::DEFAULT_FLATNESS).length();
        }
        let mut last_point = first.endpoint;
        let mut length = 0.0;
        for segment in &self.segments[1..] {
            if segment.kind == BezierSegmentKind::LineTo || segment.kind == BezierSegmentKind::Close
            {
                length += last_point.distance(segment.endpoint);
            }
            last_point = segment.endpoint;
        }
        length
    }

    pub fn is_equal(&self, other: &Self) -> bool {
        if self.segments.len() != other.segments.len() {
            return false;
        }
        self.segments
            .iter()
            .zip(other.segments.iter())
            .all(|(a, b)| a.matches(b))
    }

    pub fn flattened_copy(&self, flatness: f64) -> Self {
        let mut copy = self.clone();
        copy.flatten(flatness);
        copy
    }

    pub fn subdivided_copy(&self, max_segment_length: f64) -> Self {
        let mut copy = self.clone();
        copy.subdivide(max_segment_length);
        copy
    }

    pub fn transformed_copy(&self, transform: Transform3D) -> Self {
        let mut copy = self.clone();
        copy.apply(transform);
        copy
    }

    fn flattened_if_curved(&self, subdividing: bool) -> Self {
        if !self.has_curve() {
            return self.clone();
        }
        let mut copy = self.clone();
        if subdividing {
            copy.subdivide(Self::DEFAULT_SUBDIVIDE_SEGMENT_LENGTH);
        }
        copy.flatten(Self::DEFAULT_FLATNESS);
        copy
    }

    /// Cubic spline curve through `nodes` (from `BezierCore+Nodes.swift`).
    pub fn curve_through(nodes: &[Vector3], style: BezierNodeStyle) -> Self {
        assert!(nodes.len() >= 2);
        let mut core = Self::new();
        if nodes.len() == 2 {
            core.add(
                BezierSegmentKind::MoveTo,
                Vector3::ZERO,
                Vector3::ZERO,
                nodes[0],
            );
            core.add(
                BezierSegmentKind::LineTo,
                Vector3::ZERO,
                Vector3::ZERO,
                nodes[1],
            );
            return core;
        }

        let nb = nodes.len() + 2;
        let mut p = vec![Vector3::ZERO; nb];
        for (i, &node) in nodes.iter().enumerate() {
            p[i + 1] = node;
        }
        match style {
            BezierNodeStyle::OpenEnds => {
                p[0] = (-p[2]).adding_product(2.0, p[1]);
                p[nb - 1] = (-p[nb - 3]).adding_product(2.0, p[nb - 2]);
            }
            BezierNodeStyle::EndsMeet => {
                p[0] = p[nb - 3];
                p[nb - 1] = p[2];
            }
        }

        let mut h = vec![0.0; nb];
        let mut d = vec![0.0; nb];
        for i in 0..(nb - 1) {
            h[i] = (p[i + 1] - p[i]).length();
            d[i + 1] = d[i] + h[i];
        }

        let mut a = vec![0.0; nb];
        let mut c = vec![0.0; nb];
        for i in 2..(nb - 1) {
            a[i] = 2.0 * h[i - 1] / (h[i] + h[i - 1]);
        }
        for i in 1..(nb - 2) {
            c[i] = 2.0 * h[i] / (h[i] + h[i - 1]);
        }

        let mut g = vec![Vector3::ZERO; nb];
        for i in 1..(nb - 1) {
            g[i] = 6.0
                * (((p[i - 1] - p[i]) / (d[i - 1] - d[i]))
                    - ((p[i] - p[i + 1]) / (d[i] - d[i + 1])))
                / (d[i - 1] - d[i + 1]);
        }

        let b = 4.0;
        let mut bet = 4.0;
        let mut gam = vec![0.0; nb];
        let mut cv = vec![Vector3::ZERO; nb];
        cv[1] = g[1] / b;
        for j in 2..(nb - 1) {
            gam[j] = c[j - 1] / bet;
            bet = adding_product(b, -a[j], gam[j]);
            cv[j] = g[j].adding_product(-a[j], cv[j - 1]) / bet;
        }
        for j in (1..=(nb - 2)).rev() {
            cv[j] = cv[j].adding_product(-gam[j + 1], cv[j + 1]);
        }

        let mut last_endpoint = p[1];
        core.add(
            BezierSegmentKind::MoveTo,
            Vector3::ZERO,
            Vector3::ZERO,
            last_endpoint,
        );
        for i in 1..(nb - 2) {
            let hi = h[i];
            let cc = cv[i];
            let aa = p[i];
            let dd = (cv[i + 1] - cv[i]) / (3.0 * hi);
            let bb = ((p[i + 1] - p[i]) / hi)
                .adding_product(-(hi / 3.0), cv[i + 1].adding_product(2.0, cv[i]));

            let endpoint = aa
                .adding_product(hi, bb)
                .adding_product(hi, cc * hi)
                .adding_product(hi, dd * hi * hi);
            let control1 = last_endpoint + (bb * hi) / 3.0;
            let slope = bb
                .adding_product(hi, 2.0 * cc)
                .adding_product(hi, 3.0 * dd * hi);
            let control2 = endpoint - (slope * hi) / 3.0;

            core.add(BezierSegmentKind::CurveTo, control1, control2, endpoint);
            last_endpoint = endpoint;
        }
        core
    }

    // --- BezierCore+Editing.swift ---

    pub fn reversed(&self) -> Self {
        let mut reversed = Self::new();
        let Some(&last) = self.segments.last() else {
            return reversed;
        };
        if self.segments.len() == 1 {
            debug_assert!(last.kind == BezierSegmentKind::MoveTo);
            reversed.add(
                BezierSegmentKind::MoveTo,
                Vector3::ZERO,
                Vector3::ZERO,
                last.endpoint,
            );
            return reversed;
        }
        let mut needs_close = false;
        let mut needs_move = true;
        let mut previous = last;
        for index in (0..=(self.segments.len() - 2)).rev() {
            let segment = self.segments[index];
            if needs_move && previous.kind != BezierSegmentKind::Close {
                reversed.add(
                    BezierSegmentKind::MoveTo,
                    Vector3::ZERO,
                    Vector3::ZERO,
                    previous.endpoint,
                );
                needs_move = false;
            }
            match previous.kind {
                BezierSegmentKind::Close => {
                    needs_close = true;
                }
                BezierSegmentKind::LineTo => {
                    reversed.add(
                        BezierSegmentKind::LineTo,
                        Vector3::ZERO,
                        Vector3::ZERO,
                        segment.endpoint,
                    );
                }
                BezierSegmentKind::CurveTo => {
                    reversed.add(
                        BezierSegmentKind::CurveTo,
                        previous.control2,
                        previous.control1,
                        segment.endpoint,
                    );
                }
                BezierSegmentKind::MoveTo => {
                    if needs_close {
                        reversed.add(
                            BezierSegmentKind::Close,
                            Vector3::ZERO,
                            Vector3::ZERO,
                            Vector3::ZERO,
                        );
                    }
                    reversed.add(
                        BezierSegmentKind::MoveTo,
                        Vector3::ZERO,
                        Vector3::ZERO,
                        segment.endpoint,
                    );
                    needs_close = false;
                    needs_move = true;
                }
            }
            previous = segment;
        }
        if needs_close {
            reversed.add(
                BezierSegmentKind::Close,
                Vector3::ZERO,
                Vector3::ZERO,
                Vector3::ZERO,
            );
        }
        debug_assert!(previous.kind == BezierSegmentKind::MoveTo);
        reversed
    }

    pub fn subpaths(&self) -> Vec<Self> {
        let mut subpaths: Vec<Self> = Vec::new();
        for &segment in &self.segments {
            if segment.kind == BezierSegmentKind::MoveTo {
                subpaths.push(Self::new());
            }
            if let Some(last) = subpaths.last_mut() {
                last.add_segment(segment);
            }
        }
        subpaths
    }

    pub fn clipped(&self, start_relative_position: f64, end_relative_position: f64) -> Self {
        assert!((0.0..=1.0).contains(&start_relative_position));
        assert!((0.0..=1.0).contains(&end_relative_position));
        if (start_relative_position == 0.0 && end_relative_position == 1.0)
            || (end_relative_position == 0.0 && start_relative_position == 1.0)
            || start_relative_position == end_relative_position
            || self.segments.len() < 2
        {
            return self.clone();
        }

        let flattened = self.flattened_if_curved(false);
        let length = flattened.length();
        let start_position = start_relative_position * length;
        let end_position = end_relative_position * length;
        let mut cursor = EndpointCursor::new(&flattened);
        let mut clipped = Self::new();
        let mut distance_traveled = 0.0;
        let mut segment_length = 0.0;
        let mut segment_kind = BezierSegmentKind::MoveTo;
        let mut endpoint = Vector3::ZERO;

        let mut previous_endpoint = cursor.next().endpoint;
        while !cursor.is_at_end() {
            let segment = cursor.next();
            segment_kind = segment.kind;
            endpoint = segment.endpoint;
            if segment_kind == BezierSegmentKind::LineTo || segment_kind == BezierSegmentKind::Close
            {
                segment_length = endpoint.distance(previous_endpoint);
                if segment_length != 0.0 && distance_traveled + segment_length > start_position {
                    clipped.add(
                        BezierSegmentKind::MoveTo,
                        Vector3::ZERO,
                        Vector3::ZERO,
                        previous_endpoint.interpolated(
                            endpoint,
                            (start_position - distance_traveled) / segment_length,
                        ),
                    );
                    break;
                }
                distance_traveled += segment_length;
            }
            previous_endpoint = endpoint;
        }

        if clipped.count() < 1 && start_position < end_position {
            clipped.add(
                BezierSegmentKind::MoveTo,
                Vector3::ZERO,
                Vector3::ZERO,
                endpoint,
            );
        }

        if start_position > end_position {
            if clipped.count() == 1 {
                clipped.add(
                    BezierSegmentKind::LineTo,
                    Vector3::ZERO,
                    Vector3::ZERO,
                    endpoint,
                );
            }
            let mut needs_move_to = true;
            while !cursor.is_at_end() {
                let segment = cursor.next();
                segment_kind = segment.kind;
                endpoint = segment.endpoint;
                if cursor.is_at_end() && segment_kind == BezierSegmentKind::Close {
                    clipped.add(
                        BezierSegmentKind::LineTo,
                        Vector3::ZERO,
                        Vector3::ZERO,
                        endpoint,
                    );
                    needs_move_to = false;
                } else {
                    clipped.add(segment_kind, Vector3::ZERO, Vector3::ZERO, endpoint);
                }
            }
            cursor = EndpointCursor::new(&flattened);
            previous_endpoint = cursor.next().endpoint;
            if needs_move_to {
                clipped.add(
                    BezierSegmentKind::MoveTo,
                    Vector3::ZERO,
                    Vector3::ZERO,
                    previous_endpoint,
                );
            }
            let segment = cursor.next();
            segment_kind = segment.kind;
            endpoint = segment.endpoint;
            segment_length = endpoint.distance(previous_endpoint);
            distance_traveled = 0.0;
        }

        if segment_length != 0.0 && distance_traveled + segment_length > end_position {
            clipped.add(
                BezierSegmentKind::LineTo,
                Vector3::ZERO,
                Vector3::ZERO,
                previous_endpoint.interpolated(
                    endpoint,
                    (end_position - distance_traveled) / segment_length,
                ),
            );
            return clipped;
        }
        clipped.add(segment_kind, Vector3::ZERO, Vector3::ZERO, endpoint);
        while !cursor.is_at_end() {
            let segment = cursor.next();
            segment_kind = segment.kind;
            endpoint = segment.endpoint;
            if segment_kind == BezierSegmentKind::LineTo || segment_kind == BezierSegmentKind::Close
            {
                segment_length = endpoint.distance(previous_endpoint);
                if segment_length != 0.0 && distance_traveled + segment_length > end_position {
                    clipped.add(
                        BezierSegmentKind::LineTo,
                        Vector3::ZERO,
                        Vector3::ZERO,
                        previous_endpoint.interpolated(
                            endpoint,
                            (end_position - distance_traveled) / segment_length,
                        ),
                    );
                    break;
                }
                clipped.add(segment_kind, Vector3::ZERO, Vector3::ZERO, endpoint);
                distance_traveled += segment_length;
            } else {
                clipped.add(segment_kind, Vector3::ZERO, Vector3::ZERO, endpoint);
            }
            previous_endpoint = endpoint;
        }
        clipped
    }

    pub fn signed_area(&self, normal: Vector3) -> f64 {
        let normal = normal.normalized();
        let mut signed_area = 0.0;
        for subpath in self.subpaths() {
            if subpath.segments.last().map(|s| s.kind) == Some(BezierSegmentKind::Close) {
                let flattened = subpath.flattened_if_curved(false);
                let mut cursor = EndpointCursor::new(&flattened);
                let first = cursor.next();
                debug_assert!(first.kind == BezierSegmentKind::MoveTo);
                let mut previous_endpoint = first.endpoint;
                while !cursor.is_at_end() {
                    let endpoint = cursor.next().endpoint;
                    signed_area += previous_endpoint
                        .cross_product(endpoint)
                        .dot_product(normal);
                    previous_endpoint = endpoint;
                }
            }
        }
        signed_area * 0.5
    }

    // --- BezierCore+Measuring.swift ---

    pub fn closest_point_to_vector(&self, vector: Vector3) -> Option<BezierClosestPoint> {
        if self.segments.len() < 2 {
            return None;
        }
        let flattened = self.flattened_if_curved(false);
        let mut cursor = EndpointCursor::new(&flattened);
        let mut best_distance = f64::MAX;
        let mut best_relative_position = 0.0;
        let mut best_vector = Vector3::ZERO;
        let mut traveled_distance = 0.0;

        let mut end = cursor.next().endpoint;
        while !cursor.is_at_end() {
            let start = end;
            let segment = cursor.next();
            end = segment.endpoint;

            let segment_vector = end - start;
            let translated_vector = vector - start;
            let segment_length = segment_vector.length();
            let segment_direction = segment_vector * (1.0 / segment_length);
            let projected_distance = translated_vector.dot_product(segment_direction);

            if segment.kind == BezierSegmentKind::MoveTo {
                continue;
            }
            if projected_distance >= 0.0 && projected_distance <= segment_length {
                let distance = (translated_vector - segment_direction * projected_distance).length();
                if distance < best_distance {
                    best_distance = distance;
                    best_relative_position = traveled_distance + projected_distance;
                    best_vector = start + segment_direction * projected_distance;
                }
            } else if projected_distance < 0.0 {
                let distance = start.distance(vector);
                if distance < best_distance {
                    best_distance = distance;
                    best_relative_position = traveled_distance;
                    best_vector = start;
                }
            } else {
                let distance = end.distance(vector);
                if distance < best_distance {
                    best_distance = distance;
                    best_relative_position = traveled_distance + segment_length;
                    best_vector = end;
                }
            }
            traveled_distance += segment_length;
        }
        Some(BezierClosestPoint {
            relative_position: best_relative_position / flattened.length(),
            vector: best_vector,
            distance: best_distance,
        })
    }

    pub fn closest_point_to_line(&self, line: Line3) -> Option<BezierClosestPoint> {
        if self.segments.len() < 2 {
            return None;
        }
        let flattened = self.flattened_if_curved(false);
        let mut cursor = EndpointCursor::new(&flattened);
        let mut best_distance = f64::MAX;
        let mut best_relative_position = 0.0;
        let mut traveled_distance = 0.0;
        let mut closest_point = Vector3::ZERO;

        let mut end = cursor.next().endpoint;
        let mut best_vector = end;
        while !cursor.is_at_end() {
            let start = end;
            let segment = cursor.next();
            end = segment.endpoint;
            let segment_length = start.distance(end);
            if segment_length <= 0.0 || segment.kind == BezierSegmentKind::MoveTo {
                continue;
            }

            let closest = Line3::from_points(start, end).closest_points(line);
            if let Some(on_segment) = closest.on_self {
                closest_point = on_segment;
            }
            if closest.distance < best_distance {
                let mu = (end - start).dot_product(closest_point - start)
                    / (segment_length * segment_length);
                if mu < 0.0 {
                    let distance = start.distance_to_line(line);
                    if distance < best_distance {
                        best_distance = distance;
                        best_relative_position = traveled_distance;
                        best_vector = start;
                    }
                } else if mu > 1.0 {
                    let distance = end.distance_to_line(line);
                    if distance < best_distance {
                        best_distance = distance;
                        best_relative_position = traveled_distance + segment_length;
                        best_vector = end;
                    }
                } else {
                    best_distance = closest.distance;
                    best_relative_position = adding_product(traveled_distance, segment_length, mu);
                    best_vector = closest_point;
                }
            }
            traveled_distance += segment_length;
        }
        Some(BezierClosestPoint {
            relative_position: best_relative_position / flattened.length(),
            vector: best_vector,
            distance: best_distance,
        })
    }

    pub fn length_through_segment_at(&self, index: usize, flatness: f64) -> f64 {
        assert!(index < self.segments.len());
        let mut start = Self::new();
        for &seg in &self.segments[..=index] {
            start.add_segment(seg);
        }
        start.flattened_copy(flatness).length()
    }

    pub fn segment_lengths(&self, count: usize, flatness: f64) -> Vec<f64> {
        let Some(first) = self.segments.first() else {
            return Vec::new();
        };
        if count == 0 {
            return Vec::new();
        }
        let start = first.endpoint;
        let mut lengths = vec![0.0];
        let limit = count.min(self.segments.len()).max(1);
        for segment in self.segments.iter().skip(1).take(limit.saturating_sub(1)) {
            let mut seg = Self::new();
            seg.add(BezierSegmentKind::MoveTo, Vector3::ZERO, Vector3::ZERO, start);
            seg.add_segment(*segment);
            lengths.push(seg.flattened_copy(flatness).length());
        }
        lengths
    }

    pub fn intersection_count(&self, plane: Plane3) -> usize {
        if self.segments.len() < 2 {
            return 0;
        }
        let flattened = self.flattened_if_curved(true);
        let mut cursor = EndpointCursor::new(&flattened);
        let mut count = 0;
        let mut previous_endpoint = cursor.next().endpoint;
        while !cursor.is_at_end() {
            let segment = cursor.next();
            if segment.kind != BezierSegmentKind::MoveTo
                && plane.is_between(segment.endpoint, previous_endpoint)
            {
                count += 1;
            }
            previous_endpoint = segment.endpoint;
        }
        count
    }

    pub fn intersections(&self, plane: Plane3, count: usize) -> Vec<BezierPlaneIntersection> {
        if self.segments.len() < 2 {
            return Vec::new();
        }
        let flattened = self.flattened_if_curved(true);
        let length = flattened.length();
        let mut cursor = EndpointCursor::new(&flattened);
        let mut intersections = Vec::new();
        let mut distance = 0.0;
        let mut previous_endpoint = cursor.next().endpoint;
        while !cursor.is_at_end() && intersections.len() < count {
            let segment = cursor.next();
            if plane.is_between(segment.endpoint, previous_endpoint)
                && segment.kind != BezierSegmentKind::MoveTo
            {
                let intersection =
                    Line3::from_points(previous_endpoint, segment.endpoint).intersection(plane);
                intersections.push(BezierPlaneIntersection {
                    vector: intersection,
                    relative_position: (distance + previous_endpoint.distance(intersection))
                        / length,
                });
            }
            distance += previous_endpoint.distance(segment.endpoint);
            previous_endpoint = segment.endpoint;
        }
        intersections
    }

    pub fn adding_endpoints_at_intersections(&self, plane: Plane3) -> Self {
        if self.segments.len() < 2 {
            return self.clone();
        }
        let flattened = self.flattened_if_curved(false);
        let mut cursor = EndpointCursor::new(&flattened);
        let mut core = Self::new();
        let mut previous_endpoint = cursor.next().endpoint;
        core.add(
            BezierSegmentKind::MoveTo,
            Vector3::ZERO,
            Vector3::ZERO,
            previous_endpoint,
        );
        while !cursor.is_at_end() {
            let segment = cursor.next();
            if segment.kind != BezierSegmentKind::MoveTo
                && plane.is_between(segment.endpoint, previous_endpoint)
            {
                let hit =
                    Line3::from_points(previous_endpoint, segment.endpoint).intersection(plane);
                core.add(BezierSegmentKind::LineTo, Vector3::ZERO, Vector3::ZERO, hit);
            }
            core.add(
                segment.kind,
                Vector3::ZERO,
                Vector3::ZERO,
                segment.endpoint,
            );
            previous_endpoint = segment.endpoint;
        }
        core
    }

    pub fn projected(&self, plane: Plane3) -> Self {
        let mut core = Self::new();
        let project = |vector: Vector3| -> Vector3 {
            let line = Line3::new(vector, plane.normal);
            debug_assert!(line.is_valid());
            line.intersection(plane)
        };
        for &seg in &self.segments {
            core.add(
                seg.kind,
                project(seg.control1),
                project(seg.control2),
                project(seg.endpoint),
            );
        }
        core
    }

    pub fn least_squares_plane(&self) -> Plane3 {
        let flattened = self.flattened_if_curved(false);
        let pts: Vec<Vector3> = flattened.segments.iter().map(|s| s.endpoint).collect();
        Plane3::least_squares(&pts)
    }

    pub fn mean_distance_to_plane(&self, plane: Plane3) -> f64 {
        let flattened = self.flattened_if_curved(false);
        let mut total = 0.0;
        for seg in &flattened.segments {
            total += seg.endpoint.distance_to_plane(plane);
        }
        total / (flattened.segments.len() as f64)
    }

    pub fn plane(&self) -> Option<Plane3> {
        let plane = self.least_squares_plane();
        if self.mean_distance_to_plane(plane) < 1.0 {
            Some(plane)
        } else {
            None
        }
    }

    pub fn bounding_planes(&self, normal: Vector3) -> (Plane3, Plane3) {
        assert!(!normal.is_zero());
        let mut min_z = f64::MAX;
        let mut max_z = -f64::MAX;
        let mut top = Plane3::new(Vector3::ZERO, normal.normalized());
        let mut bottom = Plane3::new(Vector3::ZERO, top.normal);
        for seg in &self.flattened_if_curved(false).segments {
            let z = seg.endpoint.dot_product(normal);
            if z < min_z {
                min_z = z;
                bottom.point = seg.endpoint;
            }
            if z > max_z {
                max_z = z;
                top.point = seg.endpoint;
            }
        }
        (top, bottom)
    }

    // --- BezierCore+Sampling.swift ---

    pub fn vector_at_start(&self) -> Vector3 {
        self.segments
            .first()
            .map(|s| s.endpoint)
            .unwrap_or(Vector3::ZERO)
    }

    pub fn vector_at_end(&self) -> Vector3 {
        self.segments
            .last()
            .map(|s| s.endpoint)
            .unwrap_or(Vector3::ZERO)
    }

    pub fn tangent_at_start(&self) -> Vector3 {
        if self.segments.len() < 2 {
            return Vector3::ZERO;
        }
        let second = self.segments[1];
        let ref_pt = if second.kind == BezierSegmentKind::CurveTo {
            second.control1
        } else {
            self.segments[0].endpoint
        };
        (second.endpoint - ref_pt).normalized()
    }

    pub fn tangent_at_end(&self) -> Vector3 {
        if self.segments.len() < 2 {
            return Vector3::ZERO;
        }
        let last = self.segments[self.segments.len() - 1];
        let ref_pt = if last.kind == BezierSegmentKind::CurveTo {
            last.control2
        } else {
            self.segments[self.segments.len() - 2].endpoint
        };
        (last.endpoint - ref_pt).normalized()
    }

    fn start_normal(initial_normal: Vector3, direction: Vector3) -> Vector3 {
        let mut normal = (initial_normal - initial_normal.projected(direction)).normalized();
        if normal == Vector3::ZERO {
            normal = Vector3::new(-1.0, 0.0, 0.0)
                .cross_product(direction)
                .normalized();
            if normal == Vector3::ZERO {
                normal = Vector3::new(0.0, 1.0, 0.0)
                    .cross_product(direction)
                    .normalized();
            }
        }
        normal
    }

    pub fn normal_at_end(&self, initial_normal: Vector3) -> Vector3 {
        debug_assert!(self.subpath_count() == 1);
        if self.segments.len() < 2 {
            return initial_normal;
        }
        let flattened = self.flattened_if_curved(false);
        let mut cursor = EndpointCursor::new(&flattened);
        let mut start = cursor.next().endpoint;
        let mut end = cursor.next().endpoint;
        let mut previous_segment = end - start;
        let mut normal = Self::start_normal(initial_normal, previous_segment);
        while !cursor.is_at_end() {
            start = end;
            end = cursor.next().endpoint;
            let segment = end - start;
            normal = normal.bent(previous_segment, segment);
            normal = (normal - normal.projected(segment)).normalized();
            previous_segment = segment;
        }
        normal
    }

    #[allow(clippy::too_many_arguments)]
    pub fn samples(
        &self,
        spacing: f64,
        starting_distance: f64,
        initial_normal: Vector3,
        count: usize,
        wants_vectors: bool,
        wants_tangents: bool,
        wants_normals: bool,
    ) -> BezierSamples {
        let mut samples = BezierSamples::default();
        if count == 0 || self.segments.len() < 2 {
            return samples;
        }
        debug_assert!(self.subpath_count() == 1);

        let flattened = self.flattened_if_curved(true);
        let mut cursor = EndpointCursor::new(&flattened);
        let mut extra_distance = starting_distance;
        let mut start_vector = cursor.next().endpoint;
        let mut end_vector = cursor.next().endpoint;
        let mut segment_direction = (end_vector - start_vector).normalized();
        let mut segment_length = end_vector.distance(start_vector);

        let mut normal_vector = Self::start_normal(initial_normal, segment_direction);
        let mut previous_normal_vector = normal_vector;
        let mut tangent_vector = segment_direction;
        let mut previous_tangent_vector = tangent_vector;
        let mut done = false;

        while !done {
            let mut distance_traveled = extra_distance;
            let next_normal_vector: Vector3;
            let next_tangent_vector: Vector3;
            let next_vector: Vector3;
            let mut next_segment_direction = segment_direction;
            if cursor.is_at_end() {
                next_normal_vector = normal_vector;
                next_tangent_vector = tangent_vector;
                next_vector = end_vector;
                done = true;
            } else {
                next_vector = cursor.next().endpoint;
                next_segment_direction = (next_vector - end_vector).normalized();
                let mut bent = normal_vector.bent(segment_direction, next_segment_direction);
                bent = bent - bent.projected(next_segment_direction);
                next_normal_vector = bent.normalized();
                next_tangent_vector = next_segment_direction;
            }
            let start_normal_vector = previous_normal_vector
                .interpolated(normal_vector, 0.5)
                .normalized();
            let end_normal_vector = next_normal_vector
                .interpolated(normal_vector, 0.5)
                .normalized();
            let start_tangent_vector = previous_tangent_vector
                .interpolated(tangent_vector, 0.5)
                .normalized();
            let end_tangent_vector = next_tangent_vector
                .interpolated(tangent_vector, 0.5)
                .normalized();

            while distance_traveled < segment_length {
                if wants_vectors {
                    samples
                        .vectors
                        .push(start_vector + segment_direction * distance_traveled);
                }
                if wants_tangents {
                    samples.tangents.push(
                        start_tangent_vector
                            .interpolated(end_tangent_vector, distance_traveled / segment_length)
                            .normalized(),
                    );
                }
                if wants_normals {
                    samples.normals.push(
                        start_normal_vector
                            .interpolated(end_normal_vector, distance_traveled / segment_length)
                            .normalized(),
                    );
                }
                samples.count += 1;
                if samples.count >= count {
                    return samples;
                }
                distance_traveled += spacing;
            }

            extra_distance = distance_traveled - segment_length;
            previous_normal_vector = normal_vector;
            normal_vector = next_normal_vector;
            previous_tangent_vector = tangent_vector;
            tangent_vector = next_tangent_vector;
            segment_direction = next_segment_direction;
            start_vector = end_vector;
            end_vector = next_vector;
            segment_length = start_vector.distance(end_vector);
        }
        samples
    }
}
