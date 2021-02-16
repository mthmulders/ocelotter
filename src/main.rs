use std::path::Path;

use ocelotter_runtime::klass_parser::*;
use ocelotter_runtime::klass_repo::SharedKlassRepo;
use ocelotter_runtime::InterpLocalVars;
use ocelotter_runtime::JvmValue::*;
use ocelotter_util::file_to_bytes;
use structopt::StructOpt;
use walkdir::{DirEntry, WalkDir};

use ocelotter::exec_method;
use ocelotter_util::ZipFiles;
use options::Options;

mod options;

pub fn main() {
    // Parse any command-line arguments
    let options = Options::from_args();

    let mut repo = SharedKlassRepo::of();
    repo.bootstrap(ocelotter::exec_method);

    let fq_klass_name = options.fq_klass_name();
    let f_name = options.f_name();

    match &options.classpath {
        Some(path) => {
            match std::fs::metadata(&path) {
                Ok(md) if md.is_file() => process_classpath_file(&mut repo, path),
                Ok(md) if md.is_dir() => process_classpath_folder(&mut repo, path),
                Ok(_) => panic!("Path {} is neither file nor directory?!", path),
                Err(e) => panic!("Can't determine if {} is file or directory: {}", path, e),
            }
        }
        None => {
            // Not using a classpath, locate the class by its name
            let bytes = file_to_bytes(Path::new(&fq_klass_name))
                .expect(&format!("Problem reading {}", &fq_klass_name));
            let mut parser = OtKlassParser::of(bytes, fq_klass_name.clone());
            parser.parse();
            let k = parser.klass();
            repo.add_klass(&k);
        }
    }

    // FIXME Real main() signature required, dummying for ease of testing
    let main_str: String = f_name.clone() + ".main2:([Ljava/lang/String;)I";
    let main_klass = repo.lookup_klass(&f_name);
    let main = main_klass
        .get_method_by_name_and_desc(&main_str)
        .expect(&format!(
            "Error: Main method not found {}",
            main_str.clone()
        ));

    // FIXME Parameter passing
    let mut vars = InterpLocalVars::of(5);

    let ret = exec_method(&mut repo, &main, &mut vars)
        .map(|return_value| match return_value {
            Int { val: i } => i,
            _ => panic!("Error executing ".to_owned() + &f_name + " - non-int value returned"),
        })
        .expect(&format!("Error executing {} - no value returned", &f_name));

    println!("Ret: {}", ret);
}

fn process_classpath_file(repo: &mut SharedKlassRepo, file: &str) {
    ZipFiles::new(file)
        .into_iter()
        .filter(|f| match f {
            Ok((name, _)) if name.ends_with(".class") => true,
            _ => false,
        })
        .for_each(|z| {
            if let Ok((name, bytes)) = z {
                let mut parser = OtKlassParser::of(bytes, name);
                parser.parse();
                repo.add_klass(&parser.klass());
            }
        });
}

fn process_classpath_folder(repo: &mut SharedKlassRepo, folder: &str) {
    fn is_directory(entry: &DirEntry) -> bool {
        entry.metadata()
            .map(|md| md.is_dir())
            .unwrap_or(false)
    }
    fn is_class_file(entry: &DirEntry) -> bool {
        entry.file_name()
            .to_str()
            .map(|s| s.ends_with(".class"))
            .unwrap_or(false)
    }
    fn is_class_file_or_directory(entry: &DirEntry) -> bool {
        is_class_file(entry) || is_directory(entry)
    }
    let walker = WalkDir::new(folder).into_iter();
    for entry in walker.filter_entry(|e| is_class_file_or_directory(e)) {            
        if let Ok(entry) = entry {
            if is_class_file(&entry) {
                let path = entry.path();
                let file = std::fs::File::open(&path).expect("Can't open file");
                match std::fs::read(path) {
                    Ok(bytes) => {
                        let name = path.file_stem().unwrap().to_str().unwrap();
                        let mut parser = OtKlassParser::of(bytes, name.to_string());
                        parser.parse();
                        repo.add_klass(&parser.klass());
                    },
                    Err(err) => panic!("Can't open file {:?}: {}", path, err)
                }

            }
        }
    }
}