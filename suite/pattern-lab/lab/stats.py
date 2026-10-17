"""Balanced crossover schedules and conditional, order-stratified intervals."""
import itertools
import math
import random
import statistics
from .manifest import LabError

def schedule(ids, cycles):
    if len(ids) not in (2,3) or cycles < 2 or cycles > 10:
        raise LabError('two/three variants and 2..10 complete schedule cycles required')
    # Every permutation appears once per cycle. For A/B this gives AB, BA pairs.
    return [list(p) for _ in range(cycles) for p in itertools.permutations(ids)]

def summarize(blocks, ids, work, draws=2000):
    """Resample whole crossover blocks within each permutation stratum."""
    if work <= 0 or not blocks: raise LabError('empty work/samples')
    strata = {}
    for b in blocks:
        order=tuple(s['variant'] for s in b)
        if set(order)!=set(ids) or len(order)!=len(ids): raise LabError('incomplete block')
        if any(s['elapsed']<=0 for s in b): raise LabError('zero duration cannot form ratio')
        strata.setdefault(order,[]).append({s['variant']:s['elapsed'] for s in b})
    expected=set(itertools.permutations(ids))
    if set(strata)!=expected or len({len(v) for v in strata.values()})!=1 or min(map(len,strata.values()))<2:
        raise LabError('unbalanced order strata')
    rng=random.Random(0x4b45454c)
    out={}
    for candidate in ids[1:]:
        logs=[math.log(row[candidate]/row[ids[0]]) for rows in strata.values() for row in rows]
        boot=[]
        for _ in range(draws):
            sample=[]
            for rows in strata.values():
                for _ in rows:
                    row=rng.choice(rows)
                    sample.append(math.log(row[candidate]/row[ids[0]]))
            boot.append(math.exp(statistics.mean(sample)))
        boot.sort()
        out[candidate]={'ratio_to_first':math.exp(statistics.mean(logs)), 'conditional_95_interval':[boot[int(.025*draws)],boot[min(draws-1,int(.975*draws))]], 'interpretation':'elapsed candidate / first; paired bootstrap within fixed order strata; conditional on this host/session/protocol'}
    return {'comparisons':out,'per_variant_mean_per_work':{v:statistics.mean(s['elapsed']/work for b in blocks for s in b if s['variant']==v) for v in ids},'unit':blocks[0][0]['unit']+'/work','automatic_profit_pass':False}

def common_iterations(calibrations, target_ns, cap):
    if not calibrations or any(s['iterations']<=0 or s['wall_ns']<=0 for s in calibrations):
        raise LabError('invalid calibration duration')
    # A single pilot per variant; choose a common count for the fastest pilot.
    count=max(math.ceil(target_ns*s['iterations']/s['wall_ns']) for s in calibrations)
    if count>cap: raise LabError(f'target unattainable within max iterations {cap}')
    return max(1,count)
