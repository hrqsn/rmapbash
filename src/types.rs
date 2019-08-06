use num_traits::{cast::ToPrimitive, int::PrimInt};

#[derive(Clone, Copy, Debug)]
pub struct Edges<T> {
    pub n: T,
    pub e: T,
    pub s: T,
    pub w: T,
}

impl <T> Edges<T> {
    pub fn contains(&self, point: &Pair<T>) -> bool where T: Ord {
        point.x >= self.w && point.x <= self.e && point.z >= self.n && point.z <= self.s
    }

    pub fn size(&self) -> Pair<usize> where T: PrimInt + ToPrimitive {
        Pair {
            x: (self.e - self.w).to_usize().unwrap() + 1,
            z: (self.s - self.n).to_usize().unwrap() + 1,
        }
    }

    pub fn full(size: usize) -> Edges<usize> {
        Edges {
            n: 0,
            e: size - 1,
            s: size - 1,
            w: 0,
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub struct Pair<T> {
    pub x: T,
    pub z: T,
}
