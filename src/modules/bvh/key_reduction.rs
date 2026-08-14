use super::{BvhError, RetargetClip};

impl RetargetClip {
    pub fn reduce_keys(&mut self, tolerance: f32) -> Result<usize, BvhError> {
        if !tolerance.is_finite() || tolerance < 0.0 {
            return Err(BvhError::Mapping(
                "Key reduction tolerance must be finite and non-negative"
                    .to_owned(),
            ));
        }
        if self.times.len() < 3 {
            return Ok(0);
        }
        if self.times.iter().any(|time| !time.is_finite())
            || self.times.windows(2).any(|pair| pair[1] <= pair[0])
            || self.channels.iter().any(|channel| {
                channel.rotations.len() != self.times.len()
                    || channel.translations.as_ref().is_some_and(
                        |translations| translations.len() != self.times.len(),
                    )
            })
        {
            return Err(BvhError::Mapping(
                "Retarget clip keyframes do not match the timeline".to_owned(),
            ));
        }
        let mut keep = vec![true; self.times.len()];
        for index in 1..self.times.len() - 1 {
            let amount = (self.times[index] - self.times[index - 1])
                / (self.times[index + 1] - self.times[index - 1]);
            let removable = self.channels.iter().all(|channel| {
                let rotation = slerp(
                    channel.rotations[index - 1],
                    channel.rotations[index + 1],
                    amount,
                );
                quaternion_distance(rotation, channel.rotations[index])
                    <= tolerance
                    && channel.translations.as_ref().is_none_or(
                        |translations| {
                            let expected = lerp(
                                translations[index - 1],
                                translations[index + 1],
                                amount,
                            );
                            vector_distance(expected, translations[index])
                                <= tolerance
                        },
                    )
            });
            keep[index] = !removable;
        }
        let removed = keep.iter().filter(|value| !**value).count();
        if removed == 0 {
            return Ok(0);
        }
        self.times = self
            .times
            .iter()
            .zip(&keep)
            .filter_map(|(time, keep)| keep.then_some(*time))
            .collect();
        for channel in &mut self.channels {
            channel.rotations = channel
                .rotations
                .iter()
                .zip(&keep)
                .filter_map(|(value, keep)| keep.then_some(*value))
                .collect();
            if let Some(translations) = channel.translations.as_mut() {
                *translations = translations
                    .iter()
                    .zip(&keep)
                    .filter_map(|(value, keep)| keep.then_some(*value))
                    .collect();
            }
        }
        Ok(removed)
    }
}

fn lerp(left: [f32; 3], right: [f32; 3], amount: f32) -> [f32; 3] {
    [0, 1, 2].map(|index| left[index] + (right[index] - left[index]) * amount)
}

fn slerp(left: [f32; 4], right: [f32; 4], amount: f32) -> [f32; 4] {
    let mut right = right;
    let mut dot = left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a * b)
        .sum::<f32>();
    if dot < 0.0 {
        dot = -dot;
        right = right.map(|value| -value);
    }
    if dot > 0.9995 {
        return normalize(
            [0, 1, 2, 3].map(|index| {
                left[index] + (right[index] - left[index]) * amount
            }),
        );
    }
    let theta = dot.acos();
    let denominator = theta.sin();
    let left_weight = ((1.0 - amount) * theta).sin() / denominator;
    let right_weight = (amount * theta).sin() / denominator;
    normalize(
        [0, 1, 2, 3].map(|index| {
            left[index] * left_weight + right[index] * right_weight
        }),
    )
}

fn normalize(value: [f32; 4]) -> [f32; 4] {
    let length = value
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if length <= f32::EPSILON {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        value.map(|component| component / length)
    }
}

fn quaternion_distance(left: [f32; 4], right: [f32; 4]) -> f32 {
    1.0 - left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| a * b)
        .sum::<f32>()
        .abs()
}

fn vector_distance(left: [f32; 3], right: [f32; 3]) -> f32 {
    left.iter()
        .zip(right)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f32>()
        .sqrt()
}
