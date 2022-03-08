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

use cortex_m_rt::entry;

use stm32f4xx_hal as hal;

use hal::{pac, prelude::*, timer::Event};

#[entry]
fn main() -> ! {
    let peripheral = pac::Peripherals::take().unwrap();

    let rcc = peripheral.RCC.constrain();
    let clocks = rcc
        .cfgr
        .use_hse(8.MHz())
        .bypass_hse_oscillator()
        .sysclk(180.MHz())
        .pclk1(45.MHz()) // peripheral clock 1
        .freeze();

    // タイマの動作クロックは90MHzのはず
    // 1カウントの周波数を指定できるが、動作クロックの90MHzを前提に
    // 内部のレジスタで実現不可能な値は無理（実行時に停止する）
    let mut timer_tim2 = peripheral.TIM2.counter::<10000>(&clocks);
    timer_tim2.start(1.secs()).unwrap();

    let gpioa = peripheral.GPIOA.split();
    let mut led = gpioa.pa5.into_push_pull_output(); // 出力設定 & push-pull モード
    led.set_low(); // LED点灯

    loop {
        // 割り込みフラグが立ったらLED出力をトグル
        if timer_tim2.get_interrupt().contains(Event::Update) {
            timer_tim2.clear_interrupt(Event::Update);
            if led.is_set_high() {
                led.set_low();
            } else {
                led.set_high();
            }
        }
    }
}
