#[derive(Debug, Clone)]
pub enum App {
    Null,
    //
}

impl App {
    pub fn execute(&mut self, _op: &[u8]) -> Vec<u8> {
        Default::default()
    }
}
