// pub struct RustyLineLog<P: ExternalPrinter> {
//     level: Level,
//     printer: Arc<Mutex<P>>
// }

// impl<P: ExternalPrinter + Send> Log for RustyLineLog<P> {
//     fn enabled(&self, metadata: &log::Metadata) -> bool {
//         metadata.level() >= self.level
//     }

//     fn log(&self, record: &log::Record) {
//         self.printer.lock().unwrap().print(format!("[{}]", record.level(), record.)).unwrap();
//     }

//     fn flush(&self) {
//         todo!()
//     }
// }
