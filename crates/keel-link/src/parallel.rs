//! Scoped, cost-balanced work with results restored to input order.

#[cfg(test)]
thread_local! {
    /// A worker count that tests force on the calling thread.
    pub(crate) static FORCED_WORKERS: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

pub(crate) fn workers(cost: usize, grain: usize) -> usize {
    #[cfg(test)]
    if let Some(workers) = FORCED_WORKERS.get() {
        return workers;
    }
    if cost / grain < 2 {
        return 1;
    }
    (cost / grain)
        .min(std::thread::available_parallelism().map_or(1, |n| n.get()))
        .max(1)
}

pub(crate) fn map<T: Send, R: Send>(
    items: Vec<T>,
    cost: impl Fn(&T) -> usize,
    grain: usize,
    apply: impl Fn(T) -> R + Sync,
) -> Vec<R> {
    let total = items.iter().map(&cost).fold(0usize, usize::saturating_add);
    map_with_workers(items, cost, workers(total, grain), apply)
}

pub(crate) fn map_with_workers<T: Send, R: Send>(
    items: Vec<T>,
    cost: impl Fn(&T) -> usize,
    workers: usize,
    apply: impl Fn(T) -> R + Sync,
) -> Vec<R> {
    let workers = workers.min(items.len());
    if workers <= 1 {
        return items.into_iter().map(apply).collect();
    }
    let mut jobs: Vec<_> = items
        .into_iter()
        .enumerate()
        .map(|(i, item)| (i, cost(&item), item))
        .collect();
    jobs.sort_unstable_by_key(|(_, cost, _)| std::cmp::Reverse(*cost));
    let mut bins: Vec<_> = (0..workers).map(|_| Vec::new()).collect();
    let mut costs = vec![0usize; workers];
    for (index, cost, item) in jobs {
        let worker = (0..workers)
            .min_by_key(|&i| costs[i])
            .expect("nonempty worker set");
        costs[worker] = costs[worker].saturating_add(cost);
        bins[worker].push((index, item));
    }
    let run = |jobs: Vec<(usize, T)>| -> Vec<(usize, R)> {
        jobs.into_iter().map(|(i, item)| (i, apply(item))).collect()
    };
    let mut results = std::thread::scope(|scope| {
        let run = &run;
        let local = bins.pop().expect("nonempty worker set");
        let handles: Vec<_> = bins
            .into_iter()
            .map(|jobs| scope.spawn(move || run(jobs)))
            .collect();
        let mut results = run(local);
        for handle in handles {
            results.extend(handle.join().expect("link worker panicked"));
        }
        results
    });
    results.sort_unstable_by_key(|(i, _)| *i);
    results.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_results_preserve_owned_inputs_and_first_error() {
        for workers in [1, 2, 8] {
            let inputs = vec![
                "first".to_owned(),
                "a much longer second".to_owned(),
                "third".to_owned(),
            ];
            let result = map_with_workers(inputs, String::len, workers, Err::<(), _>);
            assert_eq!(
                result.into_iter().collect::<Result<Vec<_>, _>>(),
                Err("first".to_owned())
            );
        }
    }
}
