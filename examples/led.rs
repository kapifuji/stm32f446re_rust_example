// 基板上のLED(LD2)を点灯
// GPIO出力サンプル

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

use cortex_m_semihosting::hprintln;

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

    // 各機能へのクロック入力設定
    peripheral.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());

    // GPIOA-5 が LD2 に接続されている
    peripheral.GPIOA.odr.modify(|_, w| w.odr5().high());
    peripheral.GPIOA.moder.modify(|_, w| w.moder5().output());

    loop {
        if peripheral.GPIOA.odr.read().odr5().is_high() {
            hprintln!("LD2 is High").unwrap();
        }
    }
}
