# Auto Scrcpy
A tool to help with some tasks on my job.

## Commands
List the current devices running scrcpy.
```
devices
```
Restart the process for a specific device.
```
restart [serial number]
```

Finish all process and quit the execution.
```
quit
```

## Build
```
cargo build --release
```
This was made using only rust standard library so no dependencies or anything, also this makes 
easier for me use it in some places where there are restrictions for usage due to external
code.

