# Parameters and results are signed 64-bit integers.
fn score(x, y) = if x > y then x * 2 + y else y * 2 + x;
fn polynomial(x) = (x + 3) * (x - 2);
fn clamp(x, low, high) = if x < low then low else if x > high then high else x;
fn distance(x, y) = if x >= y then x - y else y - x;
fn answer() = 42;
