// This bundle is built against the pinned Sparkle headers only for update-enabled
// packages. Sparkle calls its user driver on the main thread. No AppKit windows.
#import <Sparkle/Sparkle.h>
#import <AppKit/AppKit.h>

typedef void (^Publish)(NSString *, NSString *, NSString *, NSUInteger);
typedef void (^Action)(void);
@interface KverUpdateDriver : NSObject <SPUUserDriver>
@property(copy) Publish publish;
@property(copy) NSString *message, *label, *version;
@property(copy) NSString *notesURL;
@property(copy) Action activate, dismiss;
@property NSUInteger generation;
@property BOOL hidden;
@end

@implementation KverUpdateDriver
- (void)emit {
    self.publish(self.hidden ? @"" : self.message ?: @"", self.label ?: @"", self.notesURL ?: @"", ++_generation);
}
- (void)notice:(NSString *)message action:(NSString *)label run:(Action)run dismiss:(Action)dismiss {
    self.message = message;
    self.label = label;
    self.activate = run;
    self.dismiss = dismiss;
    [self emit];
}
- (BOOL)reveal {
    if (!self.message) return NO;
    self.hidden = NO;
    [self emit];
    return YES;
}
- (void)respond:(NSUInteger)generation activate:(BOOL)activate {
    if (generation != self.generation) return;
    Action action = activate ? self.activate : self.dismiss;
    if (!activate) {
        self.hidden = YES;
        if (self.version)
            [NSUserDefaults.standardUserDefaults setObject:self.version forKey:@"KverDismissedUpdate"];
    }
    // Hiding progress is not cancellation. Keep its action for manual reveal.
    if (!action) { [self emit]; return; }
    self.activate = nil;
    self.dismiss = nil;
    self.message = nil;
    [self emit];
    action();
}
- (void)showUpdatePermissionRequest:(SPUUpdatePermissionRequest *)request reply:(void (^)(SUUpdatePermissionResponse *))reply {
    self.hidden = NO;
    void (^answer)(BOOL) = ^(BOOL allow) {
        reply([[SUUpdatePermissionResponse alloc] initWithAutomaticUpdateChecks:allow sendSystemProfile:NO]);
    };
    [self notice:@"Check automatically for Kver updates?" action:@"Allow checks"
             run:^{ answer(YES); } dismiss:^{ answer(NO); }];
}
- (void)showUserInitiatedUpdateCheckWithCancellation:(Action)cancel {
    self.hidden = NO;
    [self notice:@"Checking for Kver updates…" action:@"Cancel check" run:cancel dismiss:nil];
}
- (void)showUpdateFoundWithAppcastItem:(SUAppcastItem *)item state:(SPUUserUpdateState *)state reply:(void (^)(SPUUserUpdateChoice))reply {
    self.version = item.versionString;
    if (!state.userInitiated && [self.version isEqual:[NSUserDefaults.standardUserDefaults stringForKey:@"KverDismissedUpdate"]]) {
        reply(SPUUserUpdateChoiceDismiss);
        return;
    }
    NSString *message = [NSString stringWithFormat:@"Kver %@ is available.", item.displayVersionString];
    self.notesURL = item.fullReleaseNotesURL.absoluteString ?: item.releaseNotesURL.absoluteString ?:
        [@"https://github.com/JonGretar/Kver/releases/tag/v" stringByAppendingString:item.displayVersionString];
    Action later = ^{ reply(SPUUserUpdateChoiceDismiss); };
    if (item.informationOnlyUpdate) {
        NSURL *url = item.infoURL;
        BOOL web = [url.scheme.lowercaseString isEqual:@"https"];
        [self notice:message action:web ? @"View release" : nil run:web ? ^{
            [NSWorkspace.sharedWorkspace openURL:url]; later();
        } : (Action)nil dismiss:later];
    } else {
        NSString *label = state.stage == SPUUserUpdateStageInstalling ? @"Restart to update" :
            state.stage == SPUUserUpdateStageDownloaded ? @"Prepare update" : @"Download update";
        [self notice:message action:label run:^{ reply(SPUUserUpdateChoiceInstall); } dismiss:later];
    }
}
- (void)showDownloadInitiatedWithCancellation:(Action)cancel {
    [self notice:@"Downloading Kver update…" action:@"Cancel download" run:cancel dismiss:nil];
}
- (void)showDownloadDidReceiveExpectedContentLength:(uint64_t)bytes {}
- (void)showDownloadDidReceiveDataOfLength:(uint64_t)bytes {}
- (void)showDownloadDidStartExtractingUpdate {
    [self notice:@"Preparing Kver update…" action:nil run:nil dismiss:nil];
}
- (void)showExtractionReceivedProgress:(double)progress {}
- (void)showReadyToInstallAndRelaunch:(void (^)(SPUUserUpdateChoice))reply {
    [self notice:@"Kver is ready to update." action:@"Restart to update"
             run:^{ reply(SPUUserUpdateChoiceInstall); } dismiss:^{ reply(SPUUserUpdateChoiceDismiss); }];
}
- (void)showInstallingUpdateWithApplicationTerminated:(BOOL)terminated retryTerminatingApplication:(Action)retry {
    if (terminated) {
        [self notice:@"Installing Kver update…" action:nil run:nil dismiss:nil];
    } else {
        // The quit guard can decline termination more than once. Re-arm before
        // the retry, since it can synchronously advance Sparkle's session.
        __weak KverUpdateDriver *driver = self;
        [self notice:@"Quit Kver to finish updating." action:@"Retry restart" run:^{
            [driver showInstallingUpdateWithApplicationTerminated:NO retryTerminatingApplication:retry];
            retry();
        } dismiss:nil];
    }
}
- (void)showUpdateInstalledAndRelaunched:(BOOL)relaunched acknowledgement:(Action)ack {
    ack();
}
- (void)showUpdateNotFoundWithError:(NSError *)error acknowledgement:(Action)ack {
    [self notice:error.localizedDescription action:nil run:nil dismiss:ack];
}
- (void)showUpdaterError:(NSError *)error acknowledgement:(Action)ack {
    self.hidden = NO;
    [self notice:error.localizedDescription action:nil run:nil dismiss:ack];
}
- (void)showUpdateReleaseNotesWithDownloadData:(SPUDownloadData *)data {}
- (void)showUpdateReleaseNotesFailedToDownloadWithError:(NSError *)error {}
- (void)showUpdateInFocus { [self reveal]; }
- (void)dismissUpdateInstallation {
    self.version = nil;
    self.notesURL = nil;
    self.hidden = NO;
    [self notice:nil action:nil run:nil dismiss:nil];
}
@end
