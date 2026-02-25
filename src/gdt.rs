/// Global Descriptor Table (GDT) and Task State Segment (TSS).
///
/// In 64-bit mode, the GDT is needed for:
///   1. Switching between kernel/user code segments
///   2. Loading the TSS, which contains the Interrupt Stack Table (IST)
///
/// The IST lets us define separate stacks for specific interrupts.
/// This is critical for double faults — if a stack overflow causes a
/// page fault that then causes a double fault, we need a known-good stack.

use spin::Once;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

const STACK_SIZE: usize = 4096 * 5;

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

static mut DOUBLE_FAULT_STACK: Stack = Stack([0; STACK_SIZE]);

static TSS: Once<TaskStateSegment> = Once::new();
static GDT: Once<(GlobalDescriptorTable, Selectors)> = Once::new();

struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn init() {
    let tss = TSS.call_once(|| {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            let stack_start = VirtAddr::from_ptr(&raw const DOUBLE_FAULT_STACK);
            stack_start + STACK_SIZE as u64
        };
        tss
    });

    let (gdt, selectors) = GDT.call_once(|| {
        let mut gdt = GlobalDescriptorTable::new();
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(tss));
        (
            gdt,
            Selectors {
                code_selector,
                tss_selector,
            },
        )
    });

    gdt.load();

    unsafe {
        use x86_64::instructions::segmentation::{Segment, CS};
        use x86_64::instructions::tables::load_tss;
        CS::set_reg(selectors.code_selector);
        load_tss(selectors.tss_selector);
    }
}
