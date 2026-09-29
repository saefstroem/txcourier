MEMORY {
    FLASH : ORIGIN = 0x10000000, LENGTH = 2036K
    RAM : ORIGIN = 0x20000000, LENGTH = 512K
}

REGION_ALIAS("REGION_TEXT", FLASH);
REGION_ALIAS("REGION_RODATA", FLASH);
REGION_ALIAS("REGION_DATA", RAM);
REGION_ALIAS("REGION_BSS", RAM);
REGION_ALIAS("REGION_HEAP", RAM);
REGION_ALIAS("REGION_STACK", RAM);

__pending_sector = ORIGIN(FLASH) + LENGTH(FLASH);
__signed_sector = __pending_sector + 4K;
__token_table_sector = __signed_sector + 4K;

ASSERT(__token_table_sector + 4K == 0x10000000 + 2048K,
"the three storage sectors must end exactly at the top of flash");

/* The boot ROM enters at the start of flash and looks for the image
   definition in the first 4 KB, so riscv-rt's reset code (.init) is pulled
   in first, the image definition and picotool header after it, and .text
   starts where this ends. */
SECTIONS {
    .boot ORIGIN(FLASH) : {
        KEEP(*(.init));
        KEEP(*(.start_block));
        KEEP(*(.boot_info));
        . = ALIGN(4);
        _stext = .;
    } > FLASH
} INSERT BEFORE .text.dummy;

ASSERT(_stext <= ORIGIN(FLASH) + 4K,
"the image definition must sit in the first 4 KB of flash");

SECTIONS {
    .bi_entries : ALIGN(4) {
        __bi_entries_start = .;
        KEEP(*(.bi_entries));
        . = ALIGN(4);
        __bi_entries_end = .;
    } > FLASH
} INSERT AFTER .text;

/* The names rp-binary-info's header refers to. */
__sidata = _sidata;
__sdata = _sdata;
__edata = _edata;

/* Every RP2350 interrupt, defaulting to riscv-rt's spinning handler. */
INCLUDE device.x
