#![no_std]
#![no_main]
extern crate alloc;

use alloc::borrow::ToOwned;
use linked_list_allocator::LockedHeap;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

use core::{fmt::Write, panic::PanicInfo};
use ps2::{error::ControllerError, flags::ControllerConfigFlags, Controller};
use vga::writers::{Graphics320x240x256, GraphicsWriter, Text80x25, TextWriter};

mod vga_buffer;

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
    let msg = alloc::format!("Panicked: {}:{} {}", location.file(), location.line(), info.message());
    vga_buffer::WRITER.lock().write_str(&msg);

    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Initialize the heap. Must be called before ANY allocations!
    enable_sse();
    init_heap();

    rustpython_vm::Interpreter::without_stdlib(Default::default()).enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        let source = r#"6 * 7 * 2 * 5"#;
        let code_obj = vm
            .compile(source, rustpython_vm::compiler::Mode::Exec, "<embedded>".to_owned())
            .map_err(|err| vm.new_syntax_error(&err, Some(source))).unwrap();

        let result = vm.run_code_obj(code_obj, scope).unwrap();
        
        let msg = alloc::format!("{result:?}");
        vga_buffer::WRITER.lock().write_str(&msg).unwrap();

        loop {}
        //Ok(())
    });
    
    /*
    {
        let mode = Graphics320x240x256::new();
        mode.set_mode();
    }
    {
        let mut vga = vga::vga::VGA.lock();
        vga.set_memory_start(0xa0000);
        let mut bytes = [0xff_u8; 768];
        for (i, rgb) in bytes.chunks_exact_mut(3).enumerate() {
            rgb.fill(i.clamp(0, 0xff) as u8);
        }
        vga.color_palette_registers.load_palette(&bytes);
    }
    */

    //let mode = Graphics320x240x256::new();
    //mode.clear_screen(64/2);

    let mut ps2 = initialize_ps2().unwrap();

    let mut keyboard = pc_keyboard::Keyboard::new(
        pc_keyboard::ScancodeSet2::new(),
        pc_keyboard::layouts::Us104Key,
        pc_keyboard::HandleControl::MapLettersToUnicode,
    );
    //mode.clear_screen(0x55);

    panic!("Yay panicking!!!!");
    /*
    loop {
        while let Ok(byte) = ps2.read_data() {
            if let Ok(Some(event)) = keyboard.add_byte(byte) {
                if let Some(key) = keyboard.process_keyevent(event.clone()) {
                    if let pc_keyboard::DecodedKey::Unicode(c) = key {
                        let text_mode = Text80x25::new();
                        text_mode.set_mode();
                        //write_message(b"Hello world \n");
                    }

                    if let pc_keyboard::DecodedKey::RawKey(raw) = key {
                    }
                }
            }
        }

    }
    */

    loop {}
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

use x86_64::registers::control::{Cr0, Cr0Flags, Cr4, Cr4Flags};

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
    cr4.insert(Cr4Flags::OSFXSR);     // enable FXSAVE/FXRSTOR
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
