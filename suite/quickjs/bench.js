const which = scriptArgs[1];
if (which === "fib") {
  const fib = (n) => (n < 2 ? n : fib(n - 1) + fib(n - 2));
  print(fib(30));
} else if (which === "sort") {
  const a = [];
  let x = 42;
  for (let i = 0; i < 300000; i++) {
    x = (Math.imul(x, 1103515245) + 12345) >>> 1;
    a.push(x);
  }
  a.sort((p, q) => p - q);
  print(a[0], a[a.length - 1]);
} else if (which === "string") {
  const parts = [];
  for (let i = 0; i < 200000; i++) parts.push(`${i}:${"x".repeat(i % 10)}`);
  const s = parts.join(",");
  let n = 0;
  for (const m of s.matchAll(/\d+/g)) n += m[0].length;
  print(s.length, n, s.replace(/x+/g, "y").length);
}
