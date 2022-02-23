// タイマ割り込みを利用したLチカ

#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                     // use panic_abort as _; // requires nightly
                     // use panic_itm as _; // logs messages over ITM; requires ITM support
                     // use panic_semihosting as _; // logs messages to the host stderr; requires a debugger

// cortex-m コア共通の機能を提供
use cortex_m;
use cortex_m::interrupt::Mutex;

// cortex-m コア向けのスタートアップ処理を提供
// メモリの初期化から例外テーブルのシンボル登録（リセット以外はダミーの定義）まで実施してくれる。
use cortex_m_rt::entry;

// このデバイスクレートをuseすることで、割り込みベクタテーブルのシンボル定義が自動登録される。
// 依存にデバイスクレート追加後、これ無しでビルドすると、cortex-m-rtの "device" features
// 　がONになり、テーブル定義が空になるので怒られる。（OFFの時はダミーの定義入れてくれる）
// デバイスクレートの "rt" features を外せば "device" がONにされないので回避できるが、
// 　自前でテーブル定義する必要が出てくる。
//　（デバイス固有のペリフェラルアクセスだけ利用したい場合用だが、あまり使う機会は無さそう）
use stm32f4::stm32f446;

// interrupt マクロ が使えるようになる
// 割り込み関数の定義に必要
// （デフォルトは何もしないことが定義されていて、そこに上書きする感じ）
use stm32f4::stm32f446::interrupt;

use core::cell::RefCell;

// グローバル変数(メインと割り込み関数の両方でペリフェラルアクセスするため)
static PERIPHERAL: Mutex<RefCell<Option<stm32f446::Peripherals>>> = Mutex::new(RefCell::new(None));

// クロックの初期設定を実施
// SYSCLK: HSE(ST-Link 8MHz) -> PLL -> 180MHz
// APB1: 45MHz
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

    // APB1を分周（最大45MHz）
    peripheral.RCC.cfgr.modify(|_, w| w.ppre1().div4());
}

#[entry]
fn main() -> ! {
    // write は対象レジスタを全部書き換えるので注意
    // bitごとに書き換えたければ、modify
    let peripheral = stm32f446::Peripherals::take().unwrap();

    config_clock(&peripheral);

    // 各機能へのクロック入力設定
    peripheral.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    peripheral.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

    // GPIOA-5 が LD2 に接続されている
    peripheral.GPIOA.odr.modify(|_, w| w.odr5().low());
    peripheral.GPIOA.moder.modify(|_, w| w.moder5().output());

    // TIM2 設定（クロックはAPB1 * 2 = 90MHz）
    peripheral.TIM2.arr.write(unsafe { |w| w.bits(10000 - 1) }); // オートリロードレジスタ（カウント値設定）
    peripheral.TIM2.psc.write(unsafe { |w| w.bits(9000) }); // プリスケーラ（何クロックで1カウントか設定）
    peripheral.TIM2.dier.modify(|_, w| w.uie().enabled()); // 更新割り込み有効化

    // 割り込み登録
    let core_peripheral = cortex_m::Peripherals::take().unwrap();
    let mut nvic = core_peripheral.NVIC;
    unsafe {
        // TIM2割り込み有効化
        cortex_m::peripheral::NVIC::unmask(stm32f446::Interrupt::TIM2);
        // 割り込み優先度を10に変更
        nvic.set_priority(stm32f446::Interrupt::TIM2, 10);
    }

    peripheral.TIM2.cr1.modify(|_, w| w.cen().enabled()); // カウント開始

    // peripheral を グローバル変数にmove(つまり、以降peripheralの操作はグローバル変数使用必須)
    cortex_m::interrupt::free(|cs| PERIPHERAL.borrow(cs).replace(Some(peripheral)));

    loop {}
}

#[interrupt]
fn TIM2() {
    cortex_m::interrupt::free(|cs| {
        // peripheral access
        let peripheral = PERIPHERAL.borrow(cs).borrow();
        let peripheral = peripheral.as_ref();
        if let Some(peripheral) = peripheral {
            peripheral.TIM2.sr.modify(|_, w| w.uif().clear());
            if peripheral.GPIOA.odr.read().odr5().is_high() {
                peripheral.GPIOA.odr.modify(|_, w| w.odr5().low());
            } else {
                peripheral.GPIOA.odr.modify(|_, w| w.odr5().high());
            }
        } else {
            panic!("not initialized PERIPHERAL");
        }
    });
}
