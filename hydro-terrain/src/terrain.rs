//! TerrainModel — digital elevation model with KDTree nearest-point and bilinear
//! interpolation on a regular grid.
//!
//! Approximation note: the scatter→grid build uses inverse-distance weighting (IDW)
//! over the k=4 nearest sample points instead of scipy's Delaunay-linear `griddata`.
//! On-sample and on-grid-node queries are EXACT; off-grid queries are STATISTICAL
//! (p95 abs error ≤ 0.1 m per spec R-04). Uses kiddo 4.x KDTree.

use kiddo::float::kdtree::KdTree;
use kiddo::SquaredEuclidean;

use crate::error::TerrainError;

/// A regular grid built from scatter data via IDW.
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

    /// Build a regular interpolation grid using IDW over k=4 nearest neighbors.
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

        // zs[i][j] = elevation at (xs[i], ys[j])
        let mut zs: Vec<Vec<f64>> = Vec::with_capacity(nx);

        let k = 4usize.min(self.points.len());
        const EPSILON: f64 = 1e-10;

        for &gx in &xs {
            let mut row = Vec::with_capacity(ny);
            for &gy in &ys {
                // IDW over k nearest neighbors
                let neighbors = self.kdtree.nearest_n::<SquaredEuclidean>(&[gx, gy], k);

                // If exactly on a sample point, use exact elevation
                let mut z = 0.0f64;
                let mut w_sum = 0.0f64;
                let mut exact: Option<f64> = None;

                for nn in &neighbors {
                    let sq_dist = nn.distance; // SquaredEuclidean returns d²
                    let item_idx = nn.item as usize;
                    if sq_dist < EPSILON {
                        exact = Some(self.elevations[item_idx]);
                        break;
                    }
                    let w = 1.0 / (sq_dist + EPSILON);
                    z += w * self.elevations[item_idx];
                    w_sum += w;
                }

                let cell_z = if let Some(exact_z) = exact {
                    exact_z
                } else {
                    z / w_sum
                };

                row.push(cell_z);
            }
            zs.push(row);
        }

        self.grid = Some(RegularGrid { xs, ys, zs });
        Ok(())
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
}
