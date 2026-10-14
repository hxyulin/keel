CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT, score REAL, data BLOB);
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 20000)
INSERT INTO t SELECT x, 'name' || (x * 7919 % 1000), (x % 97) / 3.0, randomblob(0) || x FROM c;
CREATE INDEX t_name ON t(name);
SELECT count(*), sum(score), avg(score), min(name), max(name) FROM t;
SELECT name, count(*), round(sum(score), 3) FROM t GROUP BY name ORDER BY 2 DESC, 1 LIMIT 5;
SELECT printf('%5.2f|%d|%s|%x', 3.14159, 42, 'abc', 255), upper('héllo'), length('héllo'), hex('ab');
SELECT json_object('a', 1, 'b', json_array(1, 2.5, 'x')), json_extract('{"k":[1,{"z":7}]}', '$.k[1].z');
SELECT group_concat(x, ','), total(x) FROM (SELECT value AS x FROM json_each('[3,1,2]') ORDER BY 1);
SELECT a.id, b.id FROM t a JOIN t b ON a.id * 2 = b.id WHERE a.id BETWEEN 100 AND 103;
SELECT 1e308 * 10, -0.0, 9223372036854775807 + 1, 7 / 2, 7 % -3, cast('12abc' AS INTEGER), typeof(1.0);
SELECT substr('hello world', 3, 5), replace('aaa', 'a', 'bb'), instr('banana', 'nan'), trim('  x  ');
SELECT datetime(0, 'unixepoch'), date('2024-02-28', '+1 day'), strftime('%j', '2024-12-31');
WITH w AS (SELECT id, score, row_number() OVER (PARTITION BY id % 3 ORDER BY score DESC, id) AS rn FROM t) SELECT id % 3, id, score FROM w WHERE rn = 1 ORDER BY 1;
BEGIN; DELETE FROM t WHERE id % 2 = 0; ROLLBACK; SELECT count(*) FROM t;
UPDATE t SET score = score * 2 WHERE id < 10; SELECT sum(score) FROM t WHERE id < 10;
PRAGMA integrity_check;
