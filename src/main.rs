use std::collections::{HashMap, HashSet};
use std::io;
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::{process::Command, thread, time::Duration};

const THREAD_WAIT: u64 = 5;

enum ShellComandos {
    Quit,
    DeviceList,
    Nothing,
    RestartDevice,
}

/* ---------------------------- */

fn process_adb_output(adb_string: &String) -> HashSet<String> {
    let mut devices: HashSet<String> = HashSet::new();
    for line in adb_string.split("\n").collect::<Vec<&str>>() {
        if line.contains("List of devices") || line.is_empty() {
            continue;
        }
        let line_split: Vec<&str> = line.split("\t").collect();
        devices.insert(String::from(line_split[0]));
    }
    devices
}

fn adb_watcher(sender: Sender<HashSet<String>>, comando: Arc<RwLock<ShellComandos>>) {
    println!("Starting adb loop...");
    let mut last_output = String::new();
    loop {
        match comando.try_read() {
            Ok(value) => match *value {
                ShellComandos::Quit => break,
                _ => (),
            },
            Err(_) => (),
        }
        let adb_output = Command::new("adb")
            .arg("devices")
            .output()
            .expect("failed to reach adb");
        let adb_string = String::from_utf8(adb_output.stdout).unwrap();
        if !adb_string.is_empty() && adb_string != last_output {
            let adb_devices = process_adb_output(&adb_string);
            last_output = adb_string;
            sender.send(adb_devices).unwrap();
        }
        thread::sleep(Duration::from_secs(THREAD_WAIT));
    }
}

/* ---------------------------- */

fn start_process(device: &str) -> Result<Child, io::Error> {
    let new_p = Command::new("scrcpy")
        .args(["-s", device, "--no-audio"])
        .stdout(Stdio::null())
        .spawn();
    new_p
}

fn scrcpy_creator(
    adb_receiver: Receiver<HashSet<String>>,
    device_receiver: Receiver<String>,
    comando: Arc<RwLock<ShellComandos>>,
) {
    println!("Starting scrcpy loop...");
    let mut processes: HashMap<String, Child> = HashMap::new();
    let mut current_devices: HashSet<String> = HashSet::new();
    let mut command_ran = false;
    loop {
        match comando.try_read() {
            Ok(value) => match *value {
                ShellComandos::DeviceList => {
                    for device in current_devices.iter() {
                        print!("{device} ");
                    }
                    println!();
                    command_ran = true;
                }
                ShellComandos::RestartDevice => {
                    let device = device_receiver.recv().unwrap();
                    if current_devices.contains(&device) {
                        let mut p = processes.remove(&device).unwrap();
                        p.kill().unwrap_or(());
                        // some time just to be sure
                        thread::sleep(Duration::from_secs(2));
                        let new_p = match start_process(device.as_str()) {
                            Ok(child) => {
                                println!("scrcpy for {device} initialized");
                                child
                            }
                            Err(_) => {
                                println!("{device} failed to initialize");
                                continue;
                            }
                        };
                        processes.insert(device.to_string(), new_p);
                        command_ran = true;
                    }
                }
                ShellComandos::Quit => {
                    for (_, p) in processes.iter_mut() {
                        p.kill().unwrap_or(());
                    }
                    break;
                }
                _ => (),
            },
            Err(_) => (),
        }

        // some workaround to deal with data updating
        if command_ran {
            let mut c = comando.write().unwrap();
            *c = ShellComandos::Nothing;
            command_ran = false;
            continue;
        }

        let new_devices = match adb_receiver.try_recv() {
            Ok(value) => value,
            Err(_) => {
                thread::sleep(Duration::from_secs(THREAD_WAIT));
                continue;
            }
        };
        for new_device in new_devices.difference(&current_devices) {
            let new_p = match start_process(new_device.as_str()) {
                Ok(child) => {
                    println!("scrcpy for {new_device} initialized");
                    child
                }
                Err(_) => {
                    println!("{new_device} failed to initialize");
                    continue;
                }
            };
            processes.insert(new_device.to_string(), new_p);
        }

        for removed_device in current_devices.difference(&new_devices) {
            processes.remove(removed_device.as_str());
            println!("{removed_device} was removed");
        }
        current_devices = new_devices;
        thread::sleep(Duration::from_secs(THREAD_WAIT));
    }
}

/* ---------------------------- */

fn shell(device_sender: Sender<String>, comando: Arc<RwLock<ShellComandos>>) {
    loop {
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                if input.is_empty() {
                    continue;
                }
            }
            _ => (),
        };
        let composed_input: Vec<&str> = input.trim().split(" ").collect();
        match composed_input[0] {
            "quit" => {
                let mut c = comando.write().unwrap();
                *c = ShellComandos::Quit;
                break;
            }
            "devices" => {
                let mut c = comando.write().unwrap();
                *c = ShellComandos::DeviceList;
            }
            "restart" => {
                let mut c = comando.write().unwrap();
                *c = ShellComandos::RestartDevice;
                device_sender.send(composed_input[1].to_owned()).unwrap_or(());
            }
            _ => continue,
        }
    }
}

/* ---------------------------- */

fn main() {
    Command::new("adb")
        .arg("start-server")
        .output()
        .expect("failed to reach adb");

    let (device_sender, device_receiver) = mpsc::channel();
    let (adb_sender, adb_receiver) = mpsc::channel();
    let comando = Arc::new(RwLock::new(ShellComandos::Nothing));

    let adb_comando = Arc::clone(&comando);
    let adb_watch = thread::spawn(move || adb_watcher(adb_sender, adb_comando));

    let scrcpy_comando = Arc::clone(&comando);
    let scrcpy_factory =
        thread::spawn(move || scrcpy_creator(adb_receiver, device_receiver, scrcpy_comando));

    shell(device_sender, comando);

    adb_watch.join().unwrap();
    scrcpy_factory.join().unwrap();
}
