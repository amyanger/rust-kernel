/// Virtual memory management: page tables and frame allocation.
///
/// x86_64 uses 4-level page tables to translate virtual to physical addresses:
///   PML4 → PDPT → PD → PT → Physical Frame
///
/// Each table has 512 entries covering progressively smaller regions:
///   PML4: 512 GiB, PDPT: 1 GiB, PD: 2 MiB, PT: 4 KiB
///
/// CR3 register points to the physical address of the PML4 table.
/// The bootloader maps all physical memory at a known virtual offset.

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use x86_64::structures::paging::{FrameAllocator, OffsetPageTable, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

/// # Safety
/// The complete physical memory must be mapped at `physical_memory_offset`.
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = unsafe { active_level_4_table(physical_memory_offset) };
    unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) }
}

unsafe fn active_level_4_table(
    physical_memory_offset: VirtAddr,
) -> &'static mut x86_64::structures::paging::PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut x86_64::structures::paging::PageTable = virt.as_mut_ptr();

    unsafe { &mut *page_table_ptr }
}

/// Frame allocator that yields usable frames from the bootloader's memory map.
pub struct BootInfoFrameAllocator {
    memory_regions: &'static MemoryRegions,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// # Safety
    /// The memory map must be valid and usable frames must be truly unused.
    pub unsafe fn init(memory_regions: &'static MemoryRegions) -> Self {
        BootInfoFrameAllocator {
            memory_regions,
            next: 0,
        }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> + '_ {
        self.memory_regions
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .map(|r| r.start..r.end)
            .flat_map(|r| r.step_by(4096))
            .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
