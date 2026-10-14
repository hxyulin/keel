local which = arg[1]
if which == "fib" then
  local function fib(n) if n < 2 then return n end return fib(n-1) + fib(n-2) end
  print(fib(32))
elseif which == "sort" then
  local t = {} local x = 42
  for i = 1, 300000 do x = (x * 1103515245 + 12345) % 2147483648; t[i] = x end
  table.sort(t) print(t[1], t[#t])
elseif which == "string" then
  local parts = {}
  for i = 1, 200000 do parts[#parts+1] = string.format("%d:%s", i, ("x"):rep(i % 10)) end
  local s = table.concat(parts, ",")
  local n = 0 for w in s:gmatch("%d+") do n = n + #w end
  print(#s, n, (s:gsub("x+", "y")):len())
elseif which == "float" then
  local s = 0.0
  for i = 1, 5000000 do s = s + math.sin(i) * math.sqrt(i) / (i + 1.5) end
  print(string.format("%.6f", s))
end
