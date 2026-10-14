CREATE TABLE t(id INTEGER PRIMARY KEY, k INTEGER, name TEXT, v REAL);
WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < 300000)
INSERT INTO t SELECT x, (x * 7919) % 10007, 'name' || (x % 1000), x / 7.0 FROM c;
CREATE INDEX t_k ON t(k);
SELECT count(*), sum(v) FROM t WHERE k BETWEEN 100 AND 5000;
SELECT name, count(*), avg(v) FROM t GROUP BY name ORDER BY 3 DESC LIMIT 3;
SELECT count(*) FROM t a JOIN t b ON a.k = b.id WHERE a.id % 10 = 0;
UPDATE t SET v = v * 2 WHERE id % 3 = 0;
SELECT sum(length(name || v)) FROM t;
