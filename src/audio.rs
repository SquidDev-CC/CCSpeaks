/// Resample an audio stream in some sample rate, producing a stream in another sample rate.
pub struct Resample<T>
where
  T: Iterator<Item = f64>,
{
  iter: T,

  ratio: f64,
  timestamp: f64,

  prev: Option<f64>,
  next: Option<f64>,
}

impl<T> Iterator for Resample<T>
where
  T: Iterator<Item = f64>,
{
  type Item = f64;

  fn next(&mut self) -> Option<Self::Item> {
    if self.timestamp >= 1.0 {
      self.prev = self.next;
      self.next = self.iter.next();
      self.timestamp -= 1.0;
    }

    let prev = match self.prev {
      Some(x) => x,
      None => {
        let value = self.iter.next()?;
        self.prev = Some(value);
        value
      }
    };

    let value = prev * self.timestamp + (1.0 - self.timestamp) * self.next.unwrap_or(0.0);
    self.timestamp += self.ratio;
    Some(value)
  }
}

const PREC: i32 = 10;

pub struct Dfpwm<T>
where
  T: Iterator<Item = i8>,
{
  iter: T,

  // Strictly speaking i8 too, but we overflow in some places, so easier to model everything like this!
  charge: i32,
  strength: i32,
  previous_bit: bool,
}

impl<T> Iterator for Dfpwm<T>
where
  T: Iterator<Item = i8>,
{
  type Item = u8;

  fn next(&mut self) -> Option<Self::Item> {
    let mut this_byte = 0;

    for i in 0..8 {
      let level = match self.iter.next() {
        Some(x) => x as i32,
        None => {
          if i == 0 {
            return None;
          }
          break;
        }
      };

      let current_bit = level > self.charge || (level == self.charge && self.charge == 127);
      let target = if current_bit { 127 } else { -128 };

      let mut next_charge =
        self.charge + ((self.strength * (target - self.charge) + (1 << (PREC - 1))) >> PREC);
      if next_charge == self.charge && next_charge != target {
        next_charge += if current_bit { 1 } else { -1 };
      }

      let z = if current_bit == self.previous_bit {
        (1 << PREC) - 1
      } else {
        0
      };

      let mut next_strength = self.strength;
      if self.strength != z {
        next_strength += if current_bit == self.previous_bit {
          1
        } else {
          -1
        };
      }
      if next_strength < 2 << (PREC - 8) {
        next_strength = 2 << (PREC - 8);
      }

      self.charge = next_charge;
      self.strength = next_strength;
      self.previous_bit = current_bit;

      this_byte = (this_byte >> 1) | if current_bit { 128 } else { 0 };
    }

    Some(this_byte)
  }
}

pub trait AudioIterator: Iterator {
  /// Resample an audio stream in some sample rate, producing a stream in another sample rate.
  fn resample(self, from: usize, to: usize) -> Resample<Self>
  where
    Self: Sized,
    Self: Iterator<Item = f64>,
  {
    Resample {
      iter: self,
      ratio: (from as f64) / (to as f64),
      timestamp: 0.0,
      prev: None,
      next: None,
    }
  }

  /// Convert an 8-bit mono audio stream to a DFPWM stream.
  fn to_dfpwm(self) -> Dfpwm<Self>
  where
    Self: Sized,
    Self: Iterator<Item = i8>,
  {
    Dfpwm { iter: self, charge: 0, strength: 0, previous_bit: false }
  }
}

impl<T: Iterator> AudioIterator for T {}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn dfpwm_encode() {
    let input: Vec<i8> = vec![
      4, 4, 3, 3, 3, 3, 3, 2, 2, 2, 2, 2, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, -1, -1, -1, -1, -1, -2, -2,
      -2, -2, -2, -3, -3, -3, -4, -4, -4, -4, -4, -5, -5, -5, -5, -5, -6, -6, -6, -7, -7, -7, -7,
      -7, -7, -7, -7, -7, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -8, -7, -7,
      -7, -7, -7, -7, -7, -7, -7, -6, -6, -6, -6, -6, -6, -6, -6, -6, -5, -5, -5, -5, -5, -5, -5,
      -4, -4, -4, -4, -4, -3, -3, -3, -3, -3, -3, -3, -2, -2, -2, -2, -2, -1, -1, -1, -1, -1, 0, 0,
      0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4,
      4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 4, 4, 4, 4, 4, 4, 4, 4, 4, 3, 3,
      3, 3, 3, 3, 3, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1, -1,
      -1, -1, -1, -1, -1, -2, -2, -2, -2, -2, -2, -2, -2, -2, -3, -3, -3, -3, -3, -3, -3, -4, -4,
      -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4, -4,
      -4, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -2, -2, -2, -2, -2, -1, -1, -1, -1, -1, 0, 0,
      0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 5, 5,
      5, 5, 5, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 5, 5, 5, 5,
      5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 4, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 2, 2, 2, 2, 2, 1, 1, 1,
      1, 1, 0, 0, 0, 0, 0, 0, 0, -1, -1, -1, -1, -1, -2, -2, -2, -2, -2, -3, -3, -3, -3, -3, -4,
      -4, -4, -4, -4, -5, -5, -5, -5, -5, -5, -5, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -7,
      -7, -7, -7, -7, -7, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -5, -5, -5, -5, -5, -5, -5,
      -5, -5, -5, -5, -4, -4, -4, -4, -4, -4, -4, -4, -4, -3, -3, -3, -3, -3, -2, -2, -2, -2, -2,
      -1, -1, -1, -1, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 3,
      3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
      4, 4, 4, 4, 4, 4, 4, 3, 3, 3, 3, 3, 3, 3, 3, 3, 2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1,
      1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1, -1, -1, -1, -1, -1, -1, -2, -2, -2, -2,
      -2, -2, -2, -2, -2, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3,
      -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -3, -2, -2, -2, -2, -2, -2, -2, -1, -1,
      -1, -1, -1, -1, -1, -1, -1, -1, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 2,
      2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
      5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 4, 4, 4, 4, 4, 4, 4, 4, 4, 3, 3, 3, 3, 3, 2, 2,
      2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -1, -1, -1, -1, -1, -2, -2,
      -2, -2, -2, -3, -3, -3, -3, -3, -4, -4, -4, -4, -4, -5, -5, -5, -5, -5, -5, -5, -5, -5, -5,
      -5, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6, -6,
      -6, -6, -6, -6, -6, -6, -5, -5, -5, -5, -5, -5, -5, -4, -4, -4, -4, -4, -4, -4, -3, -3, -3,
      -3, -3, -3, -3, -2, -2, -2, -2, -2, -2, -2, -1, -1, -1, -1, -1, -1, -1, 0, 0, 0, 0, 0, 0, 0,
      0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3, 3,
      3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
      2, 2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
      0, 0, -1, -1, -1, -1, -1, -1, -1, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2,
      -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2,
      -2, -2, -1, -1, -1, -1, -1, -1, -1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1,
      1, 2, 2, 2, 2, 2, 2, 2, 3,
    ];
    let output: Vec<i8> = vec![
      87, 74, 42, -91, -92, -108, 84, -87, -86, 86, -83, 90, -83, -43, 90, -85, -42, 106, -43, -86,
      106, -107, 42, -107, 74, -87, 74, -91, 74, -91, -86, -86, 106, 85, 107, -83, 106, -83, -83,
      86, -75, -86, 42, 85, -107, 82, 41, -91, 82, 74, 41, -107, -86, -44, -86, 86, -75, 106, -83,
      -75, -86, -75, 90, -83, -86, -86, -86, 82, -91, 74, -107, -86, 82, -87, 82, 85, 85, 85, -83,
      86, -75, -86, -43, 90, -83, 90, 85, 85, -107, 42, -91, 82, -86, 82, 74, 41, 85, -87, -86,
      -86, 106, -75, 90, -83, 86, -85, 106, -43, 106, 85, 85, 85, 85, -107, 42, 85, -86, 42, -107,
      -86, -86, -86, -86, 106, -75, -86, 86, -85,
    ];
    let output: Vec<u8> = output.into_iter().map(|x| x as u8).collect();
    let actual_output: Vec<u8> = input.into_iter().to_dfpwm().collect();

    if output != actual_output {
      panic!("Mismatch:\nExpected: {:?}\n     Got: {:?}", output, actual_output);
    }
  }
}
