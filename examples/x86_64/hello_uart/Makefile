# Makefile for building MEL x86-64 Hello UART Boot Payload
CROSS_COMPILE ?= 
AS             = $(CROSS_COMPILE)as
LD             = $(CROSS_COMPILE)ld
OBJCOPY        = $(CROSS_COMPILE)objcopy

ASFLAGS        = --64
LDFLAGS        = -T linker.ld -nostdlib

all: boot.bin

boot.o: boot.S
	$(AS) $(ASFLAGS) -o boot.o boot.S

boot.elf: boot.o linker.ld
	$(LD) $(LDFLAGS) -o boot.elf boot.o

boot.bin: boot.elf
	$(OBJCOPY) -O binary boot.elf boot.bin

clean:
	rm -f boot.o boot.elf boot.bin

.PHONY: all clean
