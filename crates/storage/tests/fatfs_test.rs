use mel_storage::FatFs;

#[test]
fn test_fatfs_end_to_end_lifecycle() {
    // 1. Create a 32 MB disk image formatted as FAT16
    let mut disk = FatFs::create_image(32 * 1024 * 1024, Some("MELDOS")).expect("create FAT disk");
    assert_eq!(disk.size_bytes(), 32 * 1024 * 1024);

    // 2. Inject bootloader, kernel, and system config
    let boot_bin = vec![0x90; 512]; // 512-byte dummy boot sector
    let kernel_bin = vec![0xAA; 128 * 1024]; // 128 KB kernel payload
    let config_ini = b"[mel]\nharts = 1\nmemory_mb = 128\nuart_irq = 10\n";

    FatFs::inject_file(&mut disk, "boot.bin", &boot_bin).expect("inject boot.bin");
    FatFs::inject_file(&mut disk, "sys/kernel.bin", &kernel_bin).expect("inject kernel.bin");
    FatFs::inject_file(&mut disk, "etc/config.ini", config_ini).expect("inject config.ini");

    // 3. Verify existence
    assert!(FatFs::exists(&mut disk, "boot.bin").unwrap());
    assert!(FatFs::exists(&mut disk, "/sys/kernel.bin").unwrap());
    assert!(FatFs::exists(&mut disk, "\\etc\\config.ini").unwrap());
    assert!(!FatFs::exists(&mut disk, "nonexistent.txt").unwrap());

    // 4. Extract and assert bit-for-bit exactness
    let extracted_boot = FatFs::extract_file(&mut disk, "boot.bin").expect("extract boot.bin");
    assert_eq!(extracted_boot, boot_bin);

    let extracted_kernel =
        FatFs::extract_file(&mut disk, "sys/kernel.bin").expect("extract kernel.bin");
    assert_eq!(extracted_kernel, kernel_bin);

    let extracted_config =
        FatFs::extract_file(&mut disk, "etc/config.ini").expect("extract config.ini");
    assert_eq!(extracted_config, config_ini);

    // 5. List root and subdirectory contents
    let root_entries = FatFs::list_dir(&mut disk, "/").expect("list root");
    let names: Vec<String> = root_entries.iter().map(|e| e.name.clone()).collect();
    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("boot.bin")));
    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("sys")));
    assert!(names.iter().any(|n| n.eq_ignore_ascii_case("etc")));

    let sys_entries = FatFs::list_dir(&mut disk, "/sys").expect("list /sys");
    assert_eq!(sys_entries.len(), 1);
    assert!(sys_entries[0].name.eq_ignore_ascii_case("kernel.bin"));
    assert_eq!(sys_entries[0].size, (128 * 1024) as u64);

    // 6. Delete file and verify removal
    FatFs::remove_file(&mut disk, "boot.bin").expect("remove boot.bin");
    assert!(!FatFs::exists(&mut disk, "boot.bin").unwrap());
}

#[test]
fn test_fatfs_cow_persistence_and_commit() {
    // Create base image and commit
    let mut disk = FatFs::create_image(8 * 1024 * 1024, Some("BASE")).expect("create image");
    FatFs::inject_file(&mut disk, "initial.txt", b"version 1.0").expect("inject initial");
    disk.commit().expect("commit base");

    assert!(!disk.is_dirty());

    // Inject file in COW mode
    FatFs::inject_file(&mut disk, "guest_output.log", b"test log output\n").expect("inject log");
    assert!(disk.is_dirty());
    assert!(FatFs::exists(&mut disk, "guest_output.log").unwrap());

    // Commit changes into base
    disk.commit().expect("commit");
    assert!(!disk.is_dirty());

    // Verify file still exists in committed base
    let log_data = FatFs::extract_file(&mut disk, "guest_output.log").expect("extract");
    assert_eq!(log_data, b"test log output\n");
}
