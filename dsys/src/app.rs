use crate::Protocol;

pub enum App {
    Null(Null),
    Echo(Echo),
}

impl App {
    pub fn execute(&mut self, op: &[u8]) -> Box<[u8]> {
        match self {
            Self::Null(app) => app.execute(op),
            Self::Echo(app) => app.execute(op),
        }
    }
}

impl Protocol<&'_ [u8]> for App {
    type Effect = Box<[u8]>;

    fn update(&mut self, event: &'_ [u8]) -> Self::Effect {
        self.execute(event)
    }
}

pub struct Null;

impl Null {
    fn execute(&mut self, _: &[u8]) -> Box<[u8]> {
        Default::default()
    }
}

pub struct Echo;

impl Echo {
    fn execute(&mut self, op: &[u8]) -> Box<[u8]> {
        op.to_owned().into()
    }
}
