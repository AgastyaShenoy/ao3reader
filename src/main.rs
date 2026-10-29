use ao3reader::AO3ReaderApp;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> Result<(), eframe::Error> {
    // Configure wgpu to specifically use DX12 and prefer LowPower (integrated GPU if available)
    let mut wgpu_options = eframe::egui_wgpu::WgpuConfiguration::default();
    wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::CreateNew(
        eframe::egui_wgpu::WgpuSetupCreateNew {
            instance_descriptor: wgpu::InstanceDescriptor {
                backends: wgpu::Backends::DX12,
                ..Default::default()
            },
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        },
    );

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 800.0])
            .with_min_inner_size([680.0, 520.0])
            .with_title("AO3 Reader — Archive of Our Own Desktop E-Reader")
            .with_transparent(false),
        vsync: true,
        wgpu_options,
        ..Default::default()
    };

    eframe::run_native(
        "AO3 Desktop Reader",
        native_options,
        Box::new(|cc| Ok(Box::new(AO3ReaderApp::new(cc)))),
    )
}
