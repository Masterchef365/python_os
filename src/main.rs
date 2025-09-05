#![allow(static_mut_refs)]
#![no_std]
#![no_main]
extern crate alloc;

use alloc::vec;
use alloc::{borrow::ToOwned, rc::Rc, string::String, vec::Vec};
use linked_list_allocator::LockedHeap;
use pc_keyboard::{layouts::Us104Key, ScancodeSet2};
use rustpython_vm::convert::ToPyObject;
use rustpython_vm::scope::Scope;
use rustpython_vm::{TryFromObject, VirtualMachine};
use alloc::format;
use x86_64::instructions::port::Port;
use x86_64::structures::port::{PortRead, PortWrite};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

use core::{cell::RefCell, fmt::Write, panic::PanicInfo};
use ps2::{error::ControllerError, flags::ControllerConfigFlags, Controller};
use vga::writers::{Graphics320x240x256, GraphicsWriter, Text80x25, TextWriter};

#[macro_use]
pub mod vga_buffer;
mod atomics;

pub fn init_heap() {
    //pub const HEAP_START: usize = 0x_4444_4444_0000;
    pub const PHYSICAL_MEMORY_OFFSET: usize = 0xFFFF800000000000;
    pub const PHYSICAL_HEAP_OFFSET: usize = 1024 * 1024 * 100;
    pub const HEAP_START: usize = PHYSICAL_MEMORY_OFFSET + PHYSICAL_HEAP_OFFSET; // + 100 MB
    pub const HEAP_SIZE: usize = (1 << 32) - PHYSICAL_HEAP_OFFSET; // Arbitrarily decide 4GB
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut _, HEAP_SIZE);
    }
}

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let location = info.location().unwrap();
    let msg = alloc::format!(
        "Panicked: {}:{} {}",
        location.file(),
        location.line(),
        info.message()
    );
    let _ = vga_buffer::WRITER.lock().write_str(&msg);

    loop {}
}

fn read_string(
    ps2: &mut Controller,
    keyboard: &mut pc_keyboard::Keyboard<Us104Key, ScancodeSet2>,
) -> String {
    let mut string = String::new();

    loop {
        while let Ok(byte) = ps2.read_data() {
            if let Ok(Some(event)) = keyboard.add_byte(byte) {
                if let Some(key) = keyboard.process_keyevent(event.clone()) {
                    if let pc_keyboard::DecodedKey::Unicode(c) = key {
                        print!("{c}");
                        if c == '\n' {
                            return string;
                        } else {
                            string.push(c);
                        }
                    }
                }
            }
        }
    }
}

fn anon_object(vm: &VirtualMachine, name: &str) -> rustpython_vm::PyObjectRef {
    let py_type = vm.builtins.get_attr("type", vm).unwrap();
    let args = (name, vm.ctx.new_tuple(vec![]), vm.ctx.new_dict());
    py_type.call(args, vm).unwrap()
}

fn install_stdout(vm: &VirtualMachine) {
    let sys = vm.import("sys", 0).unwrap();

    let stdout = anon_object(vm, "InternalStdout");

    let writer = vm.new_function("write", move |s: String| print!("{s}"));

    stdout.set_attr("write", writer, vm).unwrap();

    sys.set_attr("stdout", stdout.clone(), vm).unwrap();
}

fn install_lowlevel(vm: &VirtualMachine, scope: Scope) {
    /// Memory operations
    fn rw_dtype<T: ToPyObject + TryFromObject + Copy>(vm: &VirtualMachine, scope: Scope) {
        let tyname = core::any::type_name::<T>();
        let name = format!("read_{tyname}").leak();
        let read_byte = vm.new_function(name, move |address: u64| unsafe {
            *(address as *const T)
        });
        scope
            .globals
            .set_item(name, read_byte.into(), vm)
            .unwrap();

        let name = format!("write_{tyname}").leak();
        let write_byte = vm.new_function(name, move |address: u64, value: T| unsafe {
            *(address as *mut T) = value;
        });

        scope
            .globals
            .set_item(name, write_byte.into(), vm)
            .unwrap();
    }

    /// I/O operations
    fn rx_dtype<T: ToPyObject + TryFromObject + PortRead + PortWrite>(vm: &VirtualMachine, scope: Scope) {
        let tyname = core::any::type_name::<T>();
        let name = format!("send_{tyname}").leak();
        let send_byte = vm.new_function(name, move |port: u16, value: T| unsafe {
            Port::new(port).write(value);
        });

        scope
            .globals
            .set_item(name, send_byte.into(), vm)
            .unwrap();

        let name = format!("recv_{tyname}").leak();
        let recv_byte = vm.new_function(name, move |port: u16, value: T| unsafe {
            Port::new(port).write(value);
        });

        scope
            .globals
            .set_item(name, recv_byte.into(), vm)
            .unwrap();

    }

    rx_dtype::<u8>(vm, scope.clone());
    rx_dtype::<u16>(vm, scope.clone());
    rx_dtype::<u32>(vm, scope.clone());

    rw_dtype::<u8>(vm, scope.clone());
    rw_dtype::<u16>(vm, scope.clone());
    rw_dtype::<u32>(vm, scope.clone());
    rw_dtype::<u64>(vm, scope.clone());

    rw_dtype::<i8>(vm, scope.clone());
    rw_dtype::<i16>(vm, scope.clone());
    rw_dtype::<i32>(vm, scope.clone());
    rw_dtype::<i64>(vm, scope.clone());
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    enable_sse();

    // Initialize the heap. Must be called before ANY allocations!
    init_heap();

    println!("Starting...");
    let interpreter = rustpython_vm::Interpreter::without_stdlib(Default::default());

    let scope = interpreter.enter(|vm| vm.new_scope_with_builtins());

    let mut ps2 = initialize_ps2().unwrap();

    let mut keyboard = pc_keyboard::Keyboard::new(
        pc_keyboard::ScancodeSet2::new(),
        pc_keyboard::layouts::Us104Key,
        pc_keyboard::HandleControl::MapLettersToUnicode,
    );

    interpreter.enter(|vm| {
        install_stdout(vm);
        install_lowlevel(vm, scope.clone());
    });

    println!("RustPython v0.4.0");
    print!(">>> ");
    loop {
        let source = read_string(&mut ps2, &mut keyboard);
        let source = source.trim();

        interpreter.enter(|vm| {
            let result = vm
                .compile(
                    &source,
                    rustpython_vm::compiler::Mode::Single,
                    "<embedded>".to_owned(),
                )
                .map_err(|err| vm.new_syntax_error(&err, Some(&source)))
                .and_then(|code_obj| vm.run_code_obj(code_obj, scope.clone()));

            match result {
                Err(e) => {
                    let mut s = alloc::string::String::new();
                    vm.write_exception(&mut s, &e).unwrap();
                    println!("Exception: {s}");
                }
                Ok(v) => {
                    println!("{v:?}");
                }
            }
        });
        print!(">>> ");
    }
}

fn initialize_ps2() -> Result<Controller, ControllerError> {
    let mut controller = unsafe { Controller::new() };

    // Step 3: Disable devices
    controller.disable_keyboard()?;
    controller.disable_mouse()?;

    // Step 4: Flush data buffer
    let _ = controller.read_data();

    // Step 5: Set config
    let mut config = controller.read_config()?;
    // Disable interrupts and scancode translation
    config.set(
        ControllerConfigFlags::ENABLE_KEYBOARD_INTERRUPT
            | ControllerConfigFlags::ENABLE_MOUSE_INTERRUPT
            | ControllerConfigFlags::ENABLE_TRANSLATE,
        false,
    );
    controller.write_config(config)?;

    // Step 6: Controller self-test
    controller.test_controller()?;
    // Write config again in case of controller reset
    controller.write_config(config)?;

    // Step 7: Determine if there are 2 devices
    let has_mouse = if config.contains(ControllerConfigFlags::DISABLE_MOUSE) {
        controller.enable_mouse()?;
        config = controller.read_config()?;
        // If mouse is working, this should now be unset
        !config.contains(ControllerConfigFlags::DISABLE_MOUSE)
    } else {
        false
    };
    // Disable mouse. If there's no mouse, this is ignored
    controller.disable_mouse()?;

    // Step 8: Interface tests
    let keyboard_works = controller.test_keyboard().is_ok();
    let mouse_works = has_mouse && controller.test_mouse().is_ok();

    // Step 9 - 10: Enable and reset devices
    config = controller.read_config()?;
    if keyboard_works {
        controller.enable_keyboard()?;
        config.set(ControllerConfigFlags::DISABLE_KEYBOARD, false);
        config.set(ControllerConfigFlags::ENABLE_KEYBOARD_INTERRUPT, true);
        controller.keyboard().reset_and_self_test().unwrap();
    }
    /*
    if mouse_works {
        controller.enable_mouse()?;
        config.set(ControllerConfigFlags::DISABLE_MOUSE, false);
        config.set(ControllerConfigFlags::ENABLE_MOUSE_INTERRUPT, true);
        controller.mouse().reset_and_self_test().unwrap();
        // This will start streaming events from the mouse
        controller.mouse().enable_data_reporting().unwrap();
    }
    */

    // Write last configuration to enable devices and interrupts
    controller.write_config(config)?;

    Ok(controller)
}

use x86_64::{
    instructions::random::RdRand,
    registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags},
};

pub fn enable_sse() {
    // --- CR0 setup ---
    let mut cr0 = Cr0::read();
    cr0.remove(Cr0Flags::EMULATE_COPROCESSOR); // clear EM bit
    cr0.insert(Cr0Flags::MONITOR_COPROCESSOR); // set MP bit
    unsafe {
        Cr0::write(cr0);
    }

    // --- CR4 setup ---
    let mut cr4 = Cr4::read();
    cr4.insert(Cr4Flags::OSFXSR); // enable FXSAVE/FXRSTOR
                                  //cr4.insert(Cr4Flags::OSXMMEXCPT); // enable unmasked SIMD FP exceptions
    unsafe {
        Cr4::write(cr4);
    }

    // --- Init FP/SSE state ---
    unsafe {
        core::arch::asm!("fninit"); // reset x87 FPU
                                    // Optionally, load a default MXCSR (control/status for SSE)
        let mxcsr: u32 = 0x1F80; // all exceptions masked, round-to-nearest
        core::arch::asm!("ldmxcsr [{}]", in(reg) &mxcsr);
    }
}
