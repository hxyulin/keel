/* Independent BigInt arithmetic and object-byte oracle. No hardcoded addresses. */
const fs=require('fs'),path=require('path'),assert=require('assert/strict');
const {performance}=require('perf_hooks'),{execFileSync}=require('child_process'),os=require('os');
const dir=process.argv[2],report=JSON.parse(fs.readFileSync(path.join(dir,'manifest.json')));
const names=['scalar','wide','integer_loop','branch_switch','checksum','pointer_mutate','local_array','recursion'];
const u=x=>BigInt.asUintN(32,x),u64=x=>BigInt.asUintN(64,x);
function initial(seed){return Uint8Array.from({length:1040},(_,i)=>(Math.imul(i,17)+(seed>>>0)+129)&255);}
function oracle(name,seed,n,bytes){
 let x=BigInt(seed>>>0),nn=BigInt(n);
 if(name==='scalar')return u((u(x*1664525n+1013904223n)^(x>>13n))+nn);
 if(name==='wide'){x=(x<<32n)|u(x^0xa5a5a5a5n);return u64((u64(x*6364136223846793005n+1442695040888963407n)^(x>>33n))+nn);}
 const a=Array.from({length:16},(_,i)=>u(x+BigInt(i)));
 function rec(v,d){return d===0?v^0x5a5a5a5an:u(rec(u(v+BigInt(d)),d-1)+v);}
 for(let i=0;i<n;i++){
  if(name==='integer_loop')x=u(x*1664525n+1013904223n)^(x>>13n);
  else if(name==='branch_switch'){
   x=(x&1n)?u(x*3n+1n):x>>1n;
   switch(Number(x&3n)){case 0:x=u(x+17n);break;case 1:x^=0x9e3779b9n;break;case 2:x=u(x*5n);break;default:x=u(x-7n);}
  }else if(name==='checksum')x=u(x*33n)^BigInt(bytes[8+(i&1023)]);
  else if(name==='pointer_mutate'){const j=8+(i&1023);bytes[j]=Number((BigInt(bytes[j])+x+BigInt(i))&255n);x=u(x*33n)^BigInt(bytes[j]);}
  else if(name==='local_array'){const j=i&15;a[j]=u(a[j]*33n)^x;x=u(x+a[j]);}
  else if(name==='recursion')x=rec(x,8);
 }
 return x;
}
function wideArg(seed){const x=BigInt(seed>>>0);return BigInt.asIntN(64,(x<<32n)|(x^0xa5a5a5a5n));}
function load(mode){
 const b=fs.readFileSync(path.join(dir,mode+'.wasm'));assert(WebAssembly.validate(b));
 const m=new WebAssembly.Module(b);assert.deepEqual(WebAssembly.Module.imports(m),[]);
 const e=new WebAssembly.Instance(m).exports;assert.equal(e.memory.buffer.byteLength,65536);
 const base=e.object_offset()>>>0,buffer=e.buffer_offset()>>>0;assert.equal(buffer,base+8);assert(base+1040<=65536);
 return {e,base,view:new Uint8Array(e.memory.buffer,base,1040)};
}
const modes=['plain','basic','clang-O0','clang-O2'];
assert.deepEqual(Object.keys(report.profiles).sort(),modes.slice().sort(),'all four profiles required');
for(const mode of modes)assert.equal(report.profiles[mode].emission,'passed',`${mode} emission failed`);
const instances=Object.fromEntries(modes.map(m=>[m,load(m)]));let checks=0;
for(const mode of modes)for(const name of names)for(const seed of [0,1,0x80000000,0xffffffff])for(const n of [0,1,31,1024,16384]){
 // Bounded recursion long input still has depth eight, not input-sized recursion.
 const inst=instances[mode],before=initial(seed),wantBytes=before.slice();inst.view.set(before);
 const want=oracle(name,seed,n,wantBytes),got=inst.e[name](name==='wide'?wideArg(seed):seed,n);
 assert.equal(name==='wide'?BigInt.asUintN(64,got):BigInt(got>>>0),want,`${mode}/${name}/${seed}/${n}`);
 assert.deepEqual(inst.view,wantBytes,`${mode}/${name} exact object/canaries`);checks++;
}
const result={node:process.version,node_exec:process.execPath,node_flags:process.execArgv,validated:modes,correctness_invocations:checks,object_bytes:1040,canary_bytes:16};
if(process.argv.includes('--correctness-only')){console.log(JSON.stringify(result));process.exit(0);}
function quiet(){
 const busy=execFileSync('ps',['-axo','pid,pcpu,comm'],{encoding:'utf8'}).split('\n').slice(1).filter(l=>{const m=l.trim().split(/\s+/);return m.length>=3 && +m[1]>=50 && +m[0]!==process.pid;});
 const load=os.loadavg();return {ok:load[0]<=Math.max(1,os.cpus().length/2)&&busy.length===0,load,busy};
}
let rng=0x5eed;function random(){rng^=rng<<13;rng^=rng>>>17;rng^=rng<<5;return rng>>>0;}
function shuffled(a){a=a.slice();for(let i=a.length-1;i>0;i--){const j=random()%(i+1);[a[i],a[j]]=[a[j],a[i]];}return a;}
let sink=0n;const samples={},rounds=[],returnChecksums={};
function batch(mode,name,seed){
 const inst=instances[mode];inst.view.set(initial(seed));
 const small=name==='scalar'||name==='wide'||name==='wrapper';const count=small?20000:8;
 const n=name==='recursion'?512:16384,fn=inst.e[name==='wrapper'?'buffer_offset':name];
 let value=name==='wide'?wideArg(seed):seed;const start=performance.now();
 for(let i=0;i<count;i++)value=fn(value,n);
 const ms=performance.now()-start;sink^=typeof value==='bigint'?value:BigInt(value>>>0);
 batch.last=String(value);return ms;
}
const cases=['wrapper',...names];
// Declared fixed warmup: 12 full batches for each profile/case, no stability claim.
for(let i=0;i<12;i++)for(const mode of shuffled(modes))for(const name of cases)batch(mode,name,random());
for(let round=0;round<7;round++){
 const gate=quiet();rounds.push(gate);if(!gate.ok){result.deferred='per-round quiet gate failed';break;}
 const order=shuffled(modes),seed=random();gate.order=order;gate.seed=seed;
 for(const mode of order){samples[mode]??={};returnChecksums[mode]??={};for(const name of cases){samples[mode][name]??=[];returnChecksums[mode][name]??=[];samples[mode][name].push(batch(mode,name,seed));returnChecksums[mode][name].push(batch.last);}}
 for(const name of names)for(const mode of modes)assert.equal(returnChecksums[mode][name][round],returnChecksums[modes[0]][name][round],`timed checksum ${mode}/${name}/${round}`);
}
function stats(a){const sorted=a.slice().sort((a,b)=>a-b);return {raw_ms:a,median_ms:sorted[(sorted.length-1)>>1],range_ms:[sorted[0],sorted.at(-1)]};}
result.samples=Object.fromEntries(Object.entries(samples).map(([m,c])=>[m,Object.fromEntries(Object.entries(c).map(([n,a])=>[n,stats(a)]))]));
result.ratios={};if(!result.deferred)for(const name of cases){result.ratios[name]={};for(const m of modes)for(const reference of ['clang-O0','clang-O2'])if(result.samples[reference])result.ratios[name][m+'/'+reference]={median_ratio:result.samples[m][name].median_ms/result.samples[reference][name].median_ms,paired_raw:result.samples[m][name].raw_ms.map((v,i)=>v/result.samples[reference][name].raw_ms[i])};}
Object.assign(result,{return_checksums:returnChecksums,rounds,warmup_batches:12,round_count:7,small_batch_invocations:20000,work_batch_invocations:8,work_iterations:16384,recursion_iterations:512,recursion_depth:8,sink:sink.toString(),note:'Same Node process, seeded paired interleaving; includes JS calls, return handling and engine tiering. Wrapper getter, scalar and wide are boundary-dominated microcases. No application claim.'});
console.log(JSON.stringify(result));
