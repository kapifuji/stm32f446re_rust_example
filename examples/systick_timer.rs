#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                     // use panic_abort as _; // requires nightly
                     // use panic_itm as _; // logs messages over ITM; requires ITM support
                     // use panic_semihosting as _; // logs messages to the host stderr; requires a debugger

// cortex-m コア共通の機能を提供
use cortex_m;

// cortex-m コア向けのスタートアップ処理を提供
// メモリの初期化から例外テーブルのシンボル登録（リセット以外はダミーの定義）まで実施してくれる。
use cortex_m_rt::{entry, exception};
use cortex_m_semihosting::hprintln;

// このデバイスクレートをuseすることで、割り込みベクタテーブルのシンボル定義が自動登録される。
// 依存にデバイスクレート追加後、これ無しでビルドすると、cortex-m-rtの "device" features
// 　がONになり、テーブル定義が空になるので怒られる。（OFFの時はダミーの定義入れてくれる）
// デバイスクレートの "rt" features を外せば "device" がONにされないので回避できるが、
// 　自前でテーブル定義する必要が出てくる。
//　（デバイス固有のペリフェラルアクセスだけ利用したい場合用だが、あまり使う機会は無さそう）
use stm32f4::stm32f446;

// クロックの初期設定を実施
// SYSCLK: HSE(ST-Link 8MHz) -> PLL -> 180MHz
fn config_clock(peripheral: &stm32f4::stm32f446::Peripherals) {
    // HSEはBypassモード(ST-Linkからの 8 MHz を使える)
    peripheral.RCC.cr.modify(|_, w| w.hsebyp().bypassed());
    // HSE ON
    peripheral.RCC.cr.modify(|_, w| w.hseon().on());
    // HSE の準備完了待ち
    while peripheral.RCC.cr.read().hserdy().is_not_ready() {}

    // クロック設定
    // PLL の ソースクロックをHSE(後述の通り8MHz)とする
    peripheral.RCC.pllcfgr.modify(|_, w| w.pllsrc().hse());
    // PLL へ入るクロックを分周（1 ~ 2MHzなので、2MHzとする）
    peripheral
        .RCC
        .pllcfgr
        .modify(|_, w| unsafe { w.pllm().bits(4) });
    // 上の続きで逓倍できる(100 ~ 432MHzなので、360MHzとする)
    peripheral
        .RCC
        .pllcfgr
        .modify(|_, w| unsafe { w.plln().bits(180) });
    // PLL クロック確定(最大180MHzなので、180MHzとする)
    // ちなみにPはシステムクロック、Rは、QはUSBなど...別々に運用できる
    peripheral.RCC.pllcfgr.modify(|_, w| w.pllp().div2());

    // PLL ON
    peripheral.RCC.cr.modify(|_, w| w.pllon().on());
    // PLL の準備完了待ち
    while peripheral.RCC.cr.read().pllrdy().is_not_ready() {}

    // フラッシュの読み出し遅延設定（180MHzだと5WS）
    peripheral.FLASH.acr.modify(|_, w| w.latency().ws5());

    // PLLPをシステムクロックとして使う設定
    peripheral.RCC.cfgr.modify(|_, w| w.sw().pll());
    while peripheral.RCC.cfgr.read().sws().is_pll() == false {}
}

#[entry]
fn main() -> ! {
    let peripheral = stm32f446::Peripherals::take().unwrap();

    config_clock(&peripheral);

    let core_peripheral = cortex_m::Peripherals::take().unwrap();

    // systick interupt setting
    // systick較正値というものがあるが、これは特定のクロック時に1ms数えるのに
    // 必要なカウント数を提供している。（特定のクロックと違えば使うことはない。）
    let mut syst = core_peripheral.SYST;
    // SYSCLK or External(SYSCLK/8)
    syst.set_clock_source(cortex_m::peripheral::syst::SystClkSource::External);
    // External: 180MHz/8 = 22.5MHz -> 500ms: 11_250_000 count
    syst.set_reload(11_250_000 - 1);
    syst.enable_counter();
    syst.enable_interrupt();

    loop {}
}

#[exception]
fn SysTick() {
    hprintln!("500 ms Systick Interrupt!").unwrap();
}
