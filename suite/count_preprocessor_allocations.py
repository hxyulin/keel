"""Separate preprocessor allocation counting. Never supplies benchmark times."""
import pathlib,sys
from profile_frontend_refresh import ROOT,rawinputs,run,store,sha

def setup():
    src=ROOT/'source/crates/keel-c/examples'
    t=(src/'profile_allocations.rs').read_text().split('fn main()')[0]
    t+=r'''
fn main()->Result<(),Box<dyn std::error::Error>> {
 let args:Vec<_>=std::env::args().collect();let app=&args[1];
 let mut opts=keel_c::PreprocessOptions{sysroot:Some("/".into()),..Default::default()};
 let definitions:&[(&str,&str)]=match app.as_str(){
  "lua"=>&[("LUA_USE_LINUX","1")],
  "quickjs"=>&[("_GNU_SOURCE","1"),("CONFIG_VERSION","\"2024-01-13\""),("CONFIG_BIGNUM","1")],
  "sqlite"=>&[("SQLITE_THREADSAFE","1"),("_GNU_SOURCE","1"),("LONGDOUBLE_TYPE","double")],_=>unreachable!()};
 opts.defines=definitions.iter().map(|(a,b)|(a.to_string(),Some(b.to_string()))).collect();
 if let Some(p)=args[2..].iter().find(|p|p.ends_with("/quickjs.c")){opts.include_dirs.push(std::path::Path::new(p).parent().unwrap().into());}
 let inputs:Vec<_>=args[2..].iter().map(|p|Ok((p,std::fs::read_to_string(p)?))).collect::<Result<_,std::io::Error>>()?;
 for (p,s) in &inputs {drop(keel_c::preprocess(s,p,&opts)?);}
 for (p,s) in &inputs {
  COUNTS.with(|c|c.set(Counts{enabled:true,..Counts::default()}));
  let output=keel_c::preprocess(s,p,&opts)?;
  let n=COUNTS.with(|c|{let n=c.get();c.set(Counts::default());n});
  let bytes=output.len();drop(output);
  println!("{{\"path\":{p:?},\"alloc\":{},\"realloc\":{},\"dealloc\":{},\"requested_bytes\":{},\"output_bytes\":{bytes}}}",n.alloc,n.realloc,n.dealloc,n.requested_bytes);
 }
 Ok(())
}
'''
    (src/'preprocess_allocations_linux.rs').write_text(t)
def collect():
    b=ROOT/'target-pp-alloc/release/examples/preprocess_allocations_linux'
    store('preprocess-allocations',{'binary_sha256':sha(b),'runs':{a:run([b,a,*ps]) for a,ps in rawinputs().items()}})
if __name__=='__main__':globals()[sys.argv[1]]()
