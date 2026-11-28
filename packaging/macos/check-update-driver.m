// Smoke-check the packaged bridge without starting an updater or touching any
// installed application. Run from package.sh against the extracted candidate.
#import <Sparkle/Sparkle.h>

@protocol RibbonDriver <SPUUserDriver>
@property(copy) void (^publish)(NSString *, NSString *, NSString *, NSUInteger);
- (BOOL)reveal;
- (void)respond:(NSUInteger)generation activate:(BOOL)activate;
@end

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        if (argc != 2) return 1;
        NSBundle *bundle = [NSBundle bundleWithPath:[NSString stringWithUTF8String:argv[1]]];
        NSError *error = nil;
        if (![bundle loadAndReturnError:&error]) {
            NSLog(@"Cannot load packaged update driver: %@", error);
            return 1;
        }
        id<RibbonDriver> driver = [NSClassFromString(@"KverUpdateDriver") new];
        __block NSUInteger current = 0;
        __block BOOL visible = NO;
        __block NSUInteger cancellations = 0, retries = 0;
        driver.publish = ^(NSString *message, NSString *label, NSString *link, NSUInteger generation) {
            current = generation;
            visible = message.length > 0;
        };
        [driver showDownloadInitiatedWithCancellation:^{ cancellations++; }];
        NSUInteger downloading = current;
        [driver respond:current activate:NO];
        if (visible || cancellations || ![driver reveal] || !visible) return 1;
        [driver respond:downloading activate:YES]; // stale control
        if (cancellations) return 1;
        [driver respond:current activate:YES];
        if (cancellations != 1) return 1;

        [driver showInstallingUpdateWithApplicationTerminated:NO retryTerminatingApplication:^{ retries++; }];
        [driver respond:current activate:YES];
        [driver respond:current activate:YES];
        if (retries != 2) return 1;
        [driver dismissUpdateInstallation];
        return visible || [driver reveal];
    }
}
