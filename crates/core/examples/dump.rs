use rsword::bind::native::SessionTable;
fn main()->Result<(),Box<dyn std::error::Error>>{
 let mut s=SessionTable::default();
 let id=s.open(&std::fs::read(std::env::args().nth(1).unwrap())?,None)?;
 println!("{}",s.document(&id,None)?);
 Ok(())
}
