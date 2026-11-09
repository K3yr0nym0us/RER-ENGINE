//! Utilidades geométricas NDC compartidas (hit-test de polígonos HUD).

pub fn polygon_centroid(vertices: &[[f32; 2]]) -> [f32; 2] {
    if vertices.is_empty() {
        return [0.0, 0.0];
    }
    let n = vertices.len() as f32;
    let sx: f32 = vertices.iter().map(|v| v[0]).sum();
    let sy: f32 = vertices.iter().map(|v| v[1]).sum();
    [sx / n, sy / n]
}

pub fn point_in_polygon(ndc: [f32; 2], vertices: &[[f32; 2]]) -> bool {
    let n = vertices.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let (x, y) = (ndc[0], ndc[1]);
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (vertices[i][0], vertices[i][1]);
        let (xj, yj) = (vertices[j][0], vertices[j][1]);
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}
