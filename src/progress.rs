use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum Event {
    Stage(String),
    Progress {
        done: usize,
        total: usize,
    },
    Log(String),
    /// El proceso del juego ya está en marcha (se terminó de preparar).
    Started,
}

/// Callback de progreso compartible entre hilos/tareas.
#[derive(Clone)]
pub struct Reporter(Arc<dyn Fn(Event) + Send + Sync>);

impl Reporter {
    pub fn new<F: Fn(Event) + Send + Sync + 'static>(f: F) -> Self {
        Reporter(Arc::new(f))
    }
    /// Reporter que imprime en consola (modo CLI).
    pub fn console() -> Self {
        Reporter::new(|e| match e {
            Event::Stage(s) => eprintln!("==> {s}"),
            Event::Progress { done, total } => {
                if total > 0 && (done == total || done % 200 == 0) {
                    eprintln!("    {done}/{total}");
                }
            }
            Event::Log(l) => println!("{l}"),
            Event::Started => {}
        })
    }
    pub fn stage(&self, s: impl Into<String>) {
        (self.0)(Event::Stage(s.into()))
    }
    pub fn progress(&self, done: usize, total: usize) {
        (self.0)(Event::Progress { done, total })
    }
    /// Reenvía un evento tal cual (para envolver un reporter en otro).
    pub fn send(&self, e: Event) {
        (self.0)(e)
    }
    pub fn started(&self) {
        (self.0)(Event::Started)
    }
    pub fn log(&self, s: impl Into<String>) {
        (self.0)(Event::Log(s.into()))
    }
}
