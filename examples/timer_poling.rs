// 1秒ごとにLED点滅を繰り返す
// システムクロック変更サンプル
//   ST-Linkのクロックを外部クロックとして取り込み、PLLで逓倍して180MHzを生成
//   これをシステムクロックとして使用するように設定
// タイマ動作サンプル

#![no_std]
#![no_main]

// pick a panicking behavior
use panic_halt as _; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                     // use panic_abort as _; // requires nightly
                     // use panic_itm as _; // logs messages over ITM; requires ITM support
                     // use panic_semihosting as _; // logs messages to the host stderr; requires a debugger

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

#[entry]
fn main() -> ! {
    // write は対象レジスタを全部書き換えるので注意
    // bitごとに書き換えたければ、modify
    let peripheral = stm32f446::Peripherals::take().unwrap();

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

    // 各機能へのクロック入力設定
    peripheral.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
    peripheral.RCC.apb1enr.modify(|_, w| w.tim2en().enabled());

    // TIM2 設定（クロックは90MHz）
    peripheral.TIM2.arr.write(unsafe { |w| w.bits(10000 - 1) }); // オートリロードレジスタ（カウント値設定）
    peripheral.TIM2.psc.write(unsafe { |w| w.bits(9000) }); // プリスケーラ（何クロックで1カウントか設定）
    peripheral.TIM2.cr1.modify(|_, w| w.cen().enabled()); // カウント開始

    // GPIOA-5 が LD2 に接続されている
    peripheral.GPIOA.odr.modify(|_, w| w.odr5().low());
    peripheral.GPIOA.moder.modify(|_, w| w.moder5().output());

    loop {
        // 割り込みフラグ（オーバーフロー、アンダーフロー時に立つ）
        if peripheral.TIM2.sr.read().uif().bit_is_set() {
            peripheral.TIM2.sr.modify(|_, w| w.uif().clear());
            if peripheral.GPIOA.odr.read().odr5().is_high() {
                peripheral.GPIOA.odr.modify(|_, w| w.odr5().low());
            } else {
                peripheral.GPIOA.odr.modify(|_, w| w.odr5().high());
            }
        }
    }
}
