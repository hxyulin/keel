"""Strict version-one input boundary. JSON numbers remain Python integers."""
import json
import re
from pathlib import Path

U64 = (1 << 64) - 1
class LabError(ValueError):
    pass

def integer(x, name, low=0, high=U64):
    if type(x) is not int or not low <= x <= high:
        raise LabError(f'{name}: expected integer in [{low}, {high}]')
    return x

def fields(x, required, optional=()):
    if not isinstance(x, dict) or set(x) - set(required) - set(optional) or set(required) - set(x):
        raise LabError(f'invalid fields; expected {required}, optional {optional}')

def ident(x):
    if not isinstance(x, str) or not re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,79}', x):
        raise LabError('invalid identifier')
    return x

def source(root, value):
    if not isinstance(value, str) or not value or Path(value).is_absolute():
        raise LabError('source must be a relative path')
    p = (root / value).resolve()
    if not p.is_relative_to(root.resolve()) or not p.is_file():
        raise LabError(f'source outside lab or missing: {value}')
    return p

def unique_object(pairs):
    d = {}
    for k, v in pairs:
        if k in d:
            raise LabError(f'duplicate JSON key: {k}')
        d[k] = v
    return d

def load(path, root):
    try:
        m = json.loads(Path(path).read_text(), object_pairs_hook=unique_object)
    except (OSError, json.JSONDecodeError) as e:
        raise LabError(str(e)) from e
    fields(m, ('schema','id','arch','description','category','variants','oracle','data_words','mask','seeds','correctness_iterations','work_per_iteration','work_unit','notes'), ('mca',))
    if type(m['schema']) is not int or m['schema'] != 1:
        raise LabError('unsupported schema')
    ident(m['id'])
    if m['arch'] not in ('aarch64','x86_64') or m['category'] not in ('latency','throughput','memory','control'):
        raise LabError('invalid arch/category')
    for k in ('description', 'work_unit'):
        if not isinstance(m[k], str) or not m[k].strip():
            raise LabError(f'{k}: nonempty string required')
    if not isinstance(m['notes'], list) or not all(isinstance(x,str) for x in m['notes']):
        raise LabError('notes must be strings')
    integer(m['data_words'], 'data_words', 1, 1048576)
    integer(m['mask'], 'mask', 0, m['data_words']-1)
    integer(m['work_per_iteration'], 'work_per_iteration', 1)
    for k in ('seeds','correctness_iterations'):
        if not isinstance(m[k], list) or not 1 <= len(m[k]) <= 64:
            raise LabError(f'{k}: expected 1..64 integers')
        for v in m[k]: integer(v,k)
        if len(set(m[k])) != len(m[k]): raise LabError(f'{k}: duplicates')
    if not {0,1,2,17,257}.issubset(m['correctness_iterations']):
        raise LabError('correctness_iterations must include 0,1,2,17,257')
    if not isinstance(m['variants'],list) or not 2 <= len(m['variants']) <= 3:
        raise LabError('expected two or three variants')
    ids = []
    for v in m['variants']:
        fields(v, ('id','source','symbol'))
        ids.append(ident(v['id']))
        source(root,v['source'])
        if v['symbol'] != 'pattern_kernel': raise LabError('invalid kernel symbol')
    if len(set(ids)) != len(ids): raise LabError('duplicate variant id')
    fields(m['oracle'], ('source','symbol'))
    source(root,m['oracle']['source'])
    if m['oracle']['symbol'] != 'pattern_oracle': raise LabError('invalid oracle symbol')
    if 'mca' in m:
        if not isinstance(m['mca'], dict) or set(m['mca']) - set(ids): raise LabError('invalid mca variant')
        for v in m['mca'].values():
            fields(v, ('source',)); source(root,v['source'])
    return m
