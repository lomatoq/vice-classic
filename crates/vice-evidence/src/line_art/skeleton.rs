pub(super) fn thin(source: &[bool], width: usize, height: usize) -> Vec<bool> {
    let mut pixels = source.to_vec();
    if width < 3 || height < 3 {
        return pixels;
    }
    let iteration_limit = width.saturating_mul(height).max(1);
    for _ in 0..iteration_limit {
        let first = removable(&pixels, width, height, true);
        for index in &first {
            pixels[*index] = false;
        }
        let second = removable(&pixels, width, height, false);
        for index in &second {
            pixels[*index] = false;
        }
        if first.is_empty() && second.is_empty() {
            break;
        }
    }
    pixels
}

fn removable(pixels: &[bool], width: usize, height: usize, first: bool) -> Vec<usize> {
    let mut remove = Vec::new();
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let index = y * width + x;
            if !pixels[index] {
                continue;
            }
            let p = neighbors(pixels, width, x, y);
            let count = p.iter().filter(|value| **value).count();
            let transitions = (0..8).filter(|i| !p[*i] && p[(*i + 1) % 8]).count();
            let triples_clear = if first {
                !p[2] || !p[4] || (!p[0] && !p[6])
            } else {
                !p[0] || !p[6] || (!p[2] && !p[4])
            };
            if (2..=6).contains(&count) && transitions == 1 && triples_clear {
                remove.push(index);
            }
        }
    }
    remove
}

fn neighbors(pixels: &[bool], width: usize, x: usize, y: usize) -> [bool; 8] {
    [
        pixels[(y - 1) * width + x],
        pixels[(y - 1) * width + x + 1],
        pixels[y * width + x + 1],
        pixels[(y + 1) * width + x + 1],
        pixels[(y + 1) * width + x],
        pixels[(y + 1) * width + x - 1],
        pixels[y * width + x - 1],
        pixels[(y - 1) * width + x - 1],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_three_pixel_bar_thins_to_one_connected_centerline() {
        let mut mask = vec![false; 9 * 9];
        for y in 3..6 {
            for x in 1..8 {
                mask[y * 9 + x] = true;
            }
        }
        let skeleton = thin(&mask, 9, 9);
        assert!(skeleton.iter().filter(|value| **value).count() >= 3);
        assert!(skeleton[4 * 9 + 4]);
    }
}
