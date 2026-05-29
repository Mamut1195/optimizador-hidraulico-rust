//! TerrainModel — digital elevation model with KDTree nearest-point and bilinear
//! interpolation on a regular grid.
//!
//! Grid seeding (scatter→grid): Delaunay-linear barycentric interpolation via the
//! `spade` crate, matching scipy.griddata(method="linear"). Points outside the
//! convex hull are filled with kiddo nearest-neighbor z, matching scipy's
//! griddata(method="nearest") NaN-fill pass. On-sample and on-grid-node queries
//! are EXACT; off-grid queries are STATISTICAL (p95 abs error ≤ 0.1 m per spec R-04).
//! Uses kiddo 4.x KDTree for NN queries.

use kiddo::float::kdtree::KdTree;
use kiddo::SquaredEuclidean;
use spade::{DelaunayTriangulation, Point2, PositionInTriangulation, Triangulation};

use crate::error::TerrainError;

// ---------------------------------------------------------------------------
// Spade vertex type carrying elevation payload
// ---------------------------------------------------------------------------

/// A triangulation vertex carrying an XY position and Z elevation.
#[derive(Debug, Clone)]
struct SpadeVertex {
    position: Point2<f64>,
    z: f64,
}

impl spade::HasPosition for SpadeVertex {
    type Scalar = f64;
    fn position(&self) -> Point2<f64> {
        self.position
    }
}

// ---------------------------------------------------------------------------
// Regular grid (bilinear query layer — unchanged)
// ---------------------------------------------------------------------------

/// A regular grid built from scatter data via Delaunay-linear interpolation.
#[derive(Debug, Clone)]
struct RegularGrid {
    /// Sorted X grid coordinates.
    xs: Vec<f64>,
    /// Sorted Y grid coordinates.
    ys: Vec<f64>,
    /// Z values: `zs[i][j]` is elevation at `(xs[i], ys[j])`.
    zs: Vec<Vec<f64>>,
}

impl RegularGrid {
    /// Bilinear interpolation at (x, y).
    ///
    /// Returns `None` if (x, y) is outside grid bounds.
    /// Matches Python `RegularGridInterpolator(method="linear")`.
    fn interpolate(&self, x: f64, y: f64) -> Option<f64> {
        let i = match self.xs.partition_point(|&v| v <= x).checked_sub(1) {
            Some(i) if i + 1 < self.xs.len() => i,
            _ => return None,
        };
        let j = match self.ys.partition_point(|&v| v <= y).checked_sub(1) {
            Some(j) if j + 1 < self.ys.len() => j,
            _ => return None,
        };

        // Ensure x is within [xs[i], xs[i+1]] and y within [ys[j], ys[j+1]]
        if x < self.xs[i] || x > self.xs[i + 1] || y < self.ys[j] || y > self.ys[j + 1] {
            return None;
        }

        let tx = (x - self.xs[i]) / (self.xs[i + 1] - self.xs[i]);
        let ty = (y - self.ys[j]) / (self.ys[j + 1] - self.ys[j]);

        let z00 = self.zs[i][j];
        let z10 = self.zs[i + 1][j];
        let z01 = self.zs[i][j + 1];
        let z11 = self.zs[i + 1][j + 1];

        let z = (1.0 - tx) * (1.0 - ty) * z00
            + tx * (1.0 - ty) * z10
            + (1.0 - tx) * ty * z01
            + tx * ty * z11;

        Some(z)
    }
}

// ---------------------------------------------------------------------------
// TerrainModel
// ---------------------------------------------------------------------------

/// Digital elevation model built from survey points.
///
/// Uses kiddo KDTree for O(log n) nearest-point queries and a regular grid
/// with bilinear interpolation for O(1) elevation lookups after `build_grid`.
pub struct TerrainModel {
    /// XY coordinates of sample points.
    points: Vec<[f64; 2]>,
    /// Z elevations, parallel to `points`.
    elevations: Vec<f64>,
    /// KDTree: item = point index (stored as u64, cast from usize).
    /// Bucket size 128 to handle 100 collinear points in a regular grid.
    kdtree: KdTree<f64, u64, 2, 128, u32>,
    /// Axis-aligned bounding box (x_min, x_max, y_min, y_max).
    bounds: (f64, f64, f64, f64),
    /// Optional regular grid for bilinear queries.
    grid: Option<RegularGrid>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Replicate `np.arange(start, stop + res * 0.5, res)` without float drift.
///
/// Returns values `start + i * res` while `<= stop + res * 0.5`.
/// This matches the Python oracle's `np.arange(x_min, x_max + resolution, resolution)`.
fn arange(start: f64, stop: f64, step: f64) -> Vec<f64> {
    let mut result = Vec::new();
    let max_val = stop + step * 0.5;
    let mut i = 0usize;
    loop {
        let v = start + (i as f64) * step;
        if v > max_val {
            break;
        }
        result.push(v);
        i += 1;
    }
    result
}

/// Barycentric linear interpolation of z at query point `p` inside triangle
/// `(v0, v1, v2)` where each vertex is `[x, y, z]`.
///
/// Matches scipy's griddata(method="linear") Delaunay-linear formula.
#[inline]
fn barycentric_z(p: [f64; 2], v0: [f64; 3], v1: [f64; 3], v2: [f64; 3]) -> f64 {
    let (x, y) = (p[0], p[1]);
    let (x0, y0, z0) = (v0[0], v0[1], v0[2]);
    let (x1, y1, z1) = (v1[0], v1[1], v1[2]);
    let (x2, y2, z2) = (v2[0], v2[1], v2[2]);

    // Barycentric coordinate denominator: area of the triangle (signed)
    let denom = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2);
    if denom.abs() < f64::EPSILON {
        // Degenerate triangle — fall back to centroid average
        return (z0 + z1 + z2) / 3.0;
    }
    let w0 = ((y1 - y2) * (x - x2) + (x2 - x1) * (y - y2)) / denom;
    let w1 = ((y2 - y0) * (x - x2) + (x0 - x2) * (y - y2)) / denom;
    let w2 = 1.0 - w0 - w1;

    w0 * z0 + w1 * z1 + w2 * z2
}

// ---------------------------------------------------------------------------
// impl TerrainModel
// ---------------------------------------------------------------------------

impl TerrainModel {
    /// Create a `TerrainModel` from a list of `[x, y, z]` triples.
    ///
    /// # Errors
    ///
    /// Returns `TerrainError::EmptyTerrain` if `xyz` is empty.
    /// Returns `TerrainError::InsufficientPoints` if fewer than 3 points.
    pub fn from_xyz_list(xyz: &[[f64; 3]]) -> Result<Self, TerrainError> {
        if xyz.is_empty() {
            return Err(TerrainError::EmptyTerrain);
        }
        if xyz.len() < 3 {
            return Err(TerrainError::InsufficientPoints {
                got: xyz.len(),
                need: 3,
            });
        }

        let mut points = Vec::with_capacity(xyz.len());
        let mut elevations = Vec::with_capacity(xyz.len());
        let mut kdtree: KdTree<f64, u64, 2, 128, u32> = KdTree::new();

        let mut x_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;
        let mut y_min = f64::INFINITY;
        let mut y_max = f64::NEG_INFINITY;

        for (idx, pt) in xyz.iter().enumerate() {
            let [x, y, z] = *pt;
            points.push([x, y]);
            elevations.push(z);
            kdtree.add(&[x, y], idx as u64);
            if x < x_min {
                x_min = x;
            }
            if x > x_max {
                x_max = x;
            }
            if y < y_min {
                y_min = y;
            }
            if y > y_max {
                y_max = y;
            }
        }

        Ok(Self {
            points,
            elevations,
            kdtree,
            bounds: (x_min, x_max, y_min, y_max),
            grid: None,
        })
    }

    /// Build a regular interpolation grid using Delaunay-linear barycentric
    /// interpolation (via the `spade` crate), matching scipy.griddata(method="linear").
    ///
    /// Grid nodes outside the convex hull of the scatter (which would be NaN in
    /// scipy's griddata) are filled with nearest-neighbor z via kiddo, matching
    /// scipy's griddata(method="nearest") NaN-fill pass.
    ///
    /// After this call, `elevation_at` uses bilinear interpolation for points
    /// within bounds (faster, smoother), falling back to NN for out-of-bounds.
    ///
    /// Grid axes replicate `np.arange(x_min, x_max + resolution, resolution)`.
    pub fn build_grid(&mut self, resolution: f64) -> Result<(), TerrainError> {
        let (x_min, x_max, y_min, y_max) = self.bounds;
        let xs = arange(x_min, x_max, resolution);
        let ys = arange(y_min, y_max, resolution);

        let nx = xs.len();
        let ny = ys.len();

        // Build Delaunay triangulation of scatter points
        let mut tri: DelaunayTriangulation<SpadeVertex> = DelaunayTriangulation::new();
        for (pt, &z) in self.points.iter().zip(self.elevations.iter()) {
            // insert() returns Err only on duplicate positions; duplicates are
            // accepted by spade (the second insert replaces the first). We
            // intentionally ignore the error — duplicate survey points will
            // adopt whichever z spade retains, which is acceptable.
            let _ = tri.insert(SpadeVertex {
                position: Point2::new(pt[0], pt[1]),
                z,
            });
        }

        // Seed each grid node via Delaunay-linear barycentric interpolation.
        // Nodes outside the convex hull use kiddo NN (matching scipy's NaN fill).
        let mut zs: Vec<Vec<f64>> = Vec::with_capacity(nx);

        for &gx in &xs {
            let mut row = Vec::with_capacity(ny);
            for &gy in &ys {
                let cell_z = self.delaunay_z_or_nn(&tri, gx, gy);
                row.push(cell_z);
            }
            zs.push(row);
        }

        self.grid = Some(RegularGrid { xs, ys, zs });
        Ok(())
    }

    /// Query the Delaunay triangulation for z at (gx, gy).
    ///
    /// - `OnFace`: barycentric linear interpolation (Delaunay-linear = scipy default).
    /// - `OnEdge`: linear interpolation between the two endpoints.
    /// - `OnVertex`: exact vertex elevation.
    /// - `OutsideOfConvexHull` / `NoTriangulation`: kiddo NN fallback (matches
    ///   scipy griddata(nearest) NaN-fill pass).
    fn delaunay_z_or_nn(&self, tri: &DelaunayTriangulation<SpadeVertex>, gx: f64, gy: f64) -> f64 {
        let loc = tri.locate(Point2::new(gx, gy));
        match loc {
            PositionInTriangulation::OnFace(fh) => {
                let face = tri.face(fh);
                let [vh0, vh1, vh2] = face.vertices();
                let d0 = vh0.data();
                let d1 = vh1.data();
                let d2 = vh2.data();
                barycentric_z(
                    [gx, gy],
                    [d0.position.x, d0.position.y, d0.z],
                    [d1.position.x, d1.position.y, d1.z],
                    [d2.position.x, d2.position.y, d2.z],
                )
            }
            PositionInTriangulation::OnEdge(eh) => {
                // Linear interpolation along the edge
                let edge = tri.directed_edge(eh);
                let from = edge.from();
                let to = edge.to();
                let (x0, y0, z0) = (
                    from.data().position.x,
                    from.data().position.y,
                    from.data().z,
                );
                let (x1, y1, z1) = (to.data().position.x, to.data().position.y, to.data().z);
                let dx = x1 - x0;
                let dy = y1 - y0;
                let len_sq = dx * dx + dy * dy;
                if len_sq < f64::EPSILON {
                    return z0;
                }
                let t = ((gx - x0) * dx + (gy - y0) * dy) / len_sq;
                z0 + t * (z1 - z0)
            }
            PositionInTriangulation::OnVertex(vh) => tri.vertex(vh).data().z,
            // Outside convex hull or degenerate: nearest-neighbor z (matches
            // scipy griddata(method="nearest") NaN-cell fill in build_grid)
            PositionInTriangulation::OutsideOfConvexHull(_)
            | PositionInTriangulation::NoTriangulation => {
                let nn = self.kdtree.nearest_one::<SquaredEuclidean>(&[gx, gy]);
                self.elevations[nn.item as usize]
            }
        }
    }

    /// Return the index of the nearest sample point to (x, y).
    ///
    /// Uses kiddo `nearest_one::<SquaredEuclidean>`. EXACT on distinct grid points.
    pub fn nearest_idx(&self, x: f64, y: f64) -> usize {
        self.kdtree.nearest_one::<SquaredEuclidean>(&[x, y]).item as usize
    }

    /// Get interpolated elevation at (x, y).
    ///
    /// - If grid is built and (x, y) is within bounds: bilinear interpolation.
    /// - Otherwise: nearest-neighbor fallback (NN elevation).
    pub fn elevation_at(&self, x: f64, y: f64) -> f64 {
        if let Some(grid) = &self.grid {
            if self.is_within_bounds(x, y) {
                if let Some(z) = grid.interpolate(x, y) {
                    return z;
                }
            }
        }
        // NN fallback
        self.elevations[self.nearest_idx(x, y)]
    }

    /// Vectorized elevation query for multiple points.
    pub fn elevations_at(&self, coords: &[[f64; 2]]) -> Vec<f64> {
        coords
            .iter()
            .map(|c| self.elevation_at(c[0], c[1]))
            .collect()
    }

    /// Return `true` when (x, y) lies inside the DEM bounding box.
    pub fn is_within_bounds(&self, x: f64, y: f64) -> bool {
        let (x_min, x_max, y_min, y_max) = self.bounds;
        x >= x_min && x <= x_max && y >= y_min && y <= y_max
    }

    /// Number of survey points.
    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    /// Minimum elevation in the dataset.
    pub fn min_elevation(&self) -> f64 {
        self.elevations
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min)
    }

    /// Maximum elevation in the dataset.
    pub fn max_elevation(&self) -> f64 {
        self.elevations
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Bounding box as (x_min, x_max, y_min, y_max).
    pub fn bounds(&self) -> (f64, f64, f64, f64) {
        self.bounds
    }
}

impl std::fmt::Debug for TerrainModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerrainModel")
            .field("point_count", &self.points.len())
            .field("bounds", &self.bounds)
            .field("grid_built", &self.grid.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_model() -> TerrainModel {
        let pts = vec![
            [0.0, 0.0, 10.0],
            [1.0, 0.0, 20.0],
            [0.0, 1.0, 30.0],
            [1.0, 1.0, 40.0],
        ];
        TerrainModel::from_xyz_list(&pts).unwrap()
    }

    #[test]
    fn arange_matches_numpy() {
        // np.arange(0, 1000, 10) → 0, 10, ..., 990 (100 values)
        let result = arange(0.0, 990.0, 10.0);
        assert_eq!(result.len(), 100);
        assert_eq!(result[0], 0.0);
        assert_eq!(result[99], 990.0);
    }

    #[test]
    fn arange_with_step_boundary() {
        // np.arange(0, 990+20, 20) → 0,20,...,1000 (51 values)
        let result = arange(0.0, 990.0, 20.0);
        assert_eq!(result.len(), 51, "expected 51 values, got {}", result.len());
        assert_eq!(result[0], 0.0);
        assert_eq!(result[50], 1000.0);
    }

    #[test]
    fn from_xyz_list_empty_errors() {
        assert!(TerrainModel::from_xyz_list(&[]).is_err());
    }

    #[test]
    fn from_xyz_list_too_few_errors() {
        let pts = vec![[0.0_f64, 0.0, 1.0], [1.0, 0.0, 2.0]];
        assert!(TerrainModel::from_xyz_list(&pts).is_err());
    }

    #[test]
    fn nearest_idx_exact_on_grid() {
        let model = make_simple_model();
        // Point (0.0, 0.0) → index 0
        assert_eq!(model.nearest_idx(0.0, 0.0), 0);
        // Point (1.0, 0.0) → index 1
        assert_eq!(model.nearest_idx(1.0, 0.0), 1);
    }

    #[test]
    fn elevation_at_nn_fallback() {
        let model = make_simple_model();
        // Without grid, nearest neighbor
        let z = model.elevation_at(0.0, 0.0);
        assert!((z - 10.0).abs() < 1e-9);
    }

    #[test]
    fn build_grid_and_bilinear_on_sample() {
        let mut model = make_simple_model();
        model.build_grid(1.0).unwrap();
        // On sample points should be exact
        assert!((model.elevation_at(0.0, 0.0) - 10.0).abs() < 1e-9);
        assert!((model.elevation_at(1.0, 1.0) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn min_max_elevation() {
        let model = make_simple_model();
        assert!((model.min_elevation() - 10.0).abs() < 1e-9);
        assert!((model.max_elevation() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn bounds_correct() {
        let model = make_simple_model();
        let (x0, x1, y0, y1) = model.bounds();
        assert!((x0 - 0.0).abs() < 1e-9);
        assert!((x1 - 1.0).abs() < 1e-9);
        assert!((y0 - 0.0).abs() < 1e-9);
        assert!((y1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn delaunay_grid_on_sample_exact() {
        // With Delaunay-linear grid seeding, on-sample points must still be exact.
        let pts = vec![
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 20.0],
            [0.0, 10.0, 30.0],
            [10.0, 10.0, 40.0],
            [5.0, 5.0, 25.0],
        ];
        let mut model = TerrainModel::from_xyz_list(&pts).unwrap();
        model.build_grid(5.0).unwrap();
        // Sample points that are also grid nodes
        for pt in &pts {
            let z = model.elevation_at(pt[0], pt[1]);
            assert!(
                (z - pt[2]).abs() < 1e-6,
                "on-sample elevation mismatch at ({},{}): got {z}, expected {}",
                pt[0],
                pt[1],
                pt[2]
            );
        }
    }
}
