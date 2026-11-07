<template>
  <div
    class="mx-auto w-full relative flex flex-col justify-center pt-72 overflow-hidden"
  >
    <div class="absolute inset-0 z-0">
      <img
        :src="bannerUrl"
        class="w-full h-[24rem] object-cover blur-sm scale-105"
      />
      <div
        class="absolute inset-0 bg-gradient-to-t from-zinc-900 via-zinc-900/80 to-transparent opacity-90"
      />
      <div
        class="absolute inset-0 bg-gradient-to-r from-zinc-900/95 via-zinc-900/80 to-transparent opacity-90"
      />
    </div>

    <div class="relative z-10">
      <div class="px-8">
        <h1
          class="text-5xl text-zinc-100 font-bold font-display drop-shadow-lg"
        >
          {{ game.mName }}
        </h1>
        <p v-if="game.playtime[0]" class="mt-2 text-sm text-zinc-300">
          Playtime: {{ formatPlaytime(game.playtime[0].seconds) }}
        </p>
        <div
          class="relative"
          v-if="
            status.type === 'Installed' &&
            status.install_type.type != InstalledType.PartiallyInstalled
          "
        >
          <div
            v-if="missingEmulators.length"
            class="absolute mt-1 text-xs text-amber-200"
          >
            Required emulator missing · Update to play
          </div>
          <div
            v-else-if="!version?.userConfiguration?.enableUpdates"
            class="absolute mt-1 inline-flex items-center gap-x-1 text-xs text-zinc-400"
          >
            Version pinned
            <svg
              class="size-3 text-blue-600"
              viewBox="0 0 24 24"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M19.1835 7.80516L16.2188 4.83755C14.1921 2.8089 13.1788 1.79457 12.0904 2.03468C11.0021 2.2748 10.5086 3.62155 9.5217 6.31506L8.85373 8.1381C8.59063 8.85617 8.45908 9.2152 8.22239 9.49292C8.11619 9.61754 7.99536 9.72887 7.86251 9.82451C7.56644 10.0377 7.19811 10.1392 6.46145 10.3423C4.80107 10.8 3.97088 11.0289 3.65804 11.5721C3.5228 11.8069 3.45242 12.0735 3.45413 12.3446C3.45809 12.9715 4.06698 13.581 5.28476 14.8L6.69935 16.2163L2.22345 20.6964C1.92552 20.9946 1.92552 21.4782 2.22345 21.7764C2.52138 22.0746 3.00443 22.0746 3.30236 21.7764L7.77841 17.2961L9.24441 18.7635C10.4699 19.9902 11.0827 20.6036 11.7134 20.6045C11.9792 20.6049 12.2404 20.5358 12.4713 20.4041C13.0192 20.0914 13.2493 19.2551 13.7095 17.5825C13.9119 16.8472 14.013 16.4795 14.2254 16.1835C14.3184 16.054 14.4262 15.9358 14.5468 15.8314C14.8221 15.593 15.1788 15.459 15.8922 15.191L17.7362 14.4981C20.4 13.4973 21.7319 12.9969 21.9667 11.9115C22.2014 10.826 21.1954 9.81905 19.1835 7.80516Z"
                fill="currentColor"
              />
            </svg>
          </div>
          <div
            v-else-if="!status.update_available"
            class="absolute mt-1 inline-flex items-center gap-x-1 text-xs text-zinc-400"
          >
            Up to date <CheckCircleIcon class="size-3 text-green-600" />
          </div>
          <div
            v-else-if="status.update_available"
            class="absolute mt-1 inline-flex items-center gap-x-1 text-xs text-zinc-400"
          >
            Update available <ArrowDownTrayIcon class="size-3 text-blue-600" />
          </div>
        </div>

        <div class="mt-8 flex flex-row gap-x-4 items-stretch">
          <!-- Do not add scale animations to this: https://stackoverflow.com/a/35683068 -->
          <GameStatusButton
            @install="() => installFlow()"
            @launch="() => launch()"
            @queue="() => queue()"
            @uninstall="() => uninstall()"
            @kill="() => kill()"
            @options="() => (configureModalOpen = true)"
            @open-folder="() => openGameFolder()"
            @resume="() => resumeDownload()"
            @update="repairEmulators"
            :requires-update="
              missingEmulators.length > 0 || Boolean(dependencyCheckError)
            "
            :updating-dependencies="dependencyDownloadActive"
            :checking-dependencies="checkingDependencies"
            :status="status"
          />
          <button
            v-if="
              status.type === 'Installed' &&
              status.update_available &&
              !missingEmulators.length &&
              !dependencyCheckError
            "
            class="transition-transform duration-300 hover:scale-105 active:scale-95 inline-flex gap-x-2 items-center rounded-md bg-blue-600 px-6 font-semibold text-white shadow-xl backdrop-blur-sm hover:bg-blue-700 uppercase font-display"
            @click="() => installFlow()"
          >
            Update <ArrowDownTrayIcon class="size-5" />
          </button>
          <NuxtLink
            class="transition-transform duration-300 hover:scale-105 active:scale-95 inline-flex items-center rounded-md bg-zinc-800/50 px-6 font-semibold text-white shadow-xl backdrop-blur-sm hover:bg-zinc-800/80 uppercase font-display"
            :to="{
              path: '/store',
              query: {
                gameId: game.id,
              },
            }"
          >
            <BuildingStorefrontIcon class="mr-2 size-5" aria-hidden="true" />
            Store
          </NuxtLink>
        </div>
        <p
          v-if="dependencyCheckError"
          role="alert"
          class="mt-3 text-sm text-red-300"
        >
          {{ dependencyCheckError }}. Click Update to retry.
        </p>
        <SteamActions
          v-if="
            !missingEmulators.length &&
            !dependencyCheckError &&
            !checkingDependencies
          "
          ref="steamActions"
          class="mt-3"
          :game-id="game.id"
          :game-name="game.mName"
          @direct="launch(true)"
        />
      </div>

      <!-- Main content -->
      <div class="mt-8 w-full bg-zinc-900 px-8">
        <div class="grid grid-cols-[2fr,1fr] gap-8">
          <div class="space-y-6">
            <div class="bg-zinc-800/50 rounded-xl p-6 backdrop-blur-sm">
              <div
                v-html="htmlDescription"
                class="prose prose-invert prose-blue overflow-y-auto custom-scrollbar max-w-none"
              ></div>
            </div>
          </div>

          <div class="space-y-6">
            <div class="bg-zinc-800/50 rounded-xl p-6 backdrop-blur-sm">
              <h2 class="text-xl font-display font-semibold text-zinc-100 mb-4">
                Game Images
              </h2>
              <div class="relative">
                <div v-if="game.mImageCarouselObjectIds.length > 0">
                  <div
                    class="relative aspect-video rounded-lg overflow-hidden cursor-pointer group"
                  >
                    <div
                      class="absolute inset-0"
                      @click="
                        fullscreenImage =
                          game.mImageCarouselObjectIds[currentImageIndex]
                      "
                    >
                      <TransitionGroup name="slide" tag="div" class="h-full">
                        <img
                          v-for="(url, index) in game.mImageCarouselObjectIds"
                          :key="index"
                          :src="useObject(url)"
                          class="absolute inset-0 w-full h-full object-cover"
                          v-show="index === currentImageIndex"
                        />
                      </TransitionGroup>
                    </div>

                    <div
                      class="absolute inset-0 flex items-center justify-between px-4 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none"
                    >
                      <div class="pointer-events-auto">
                        <button
                          v-if="game.mImageCarouselObjectIds.length > 1"
                          @click.stop="previousImage()"
                          class="p-2 rounded-full bg-zinc-900/50 text-zinc-100 hover:bg-zinc-900/80 transition-all duration-300 hover:scale-110"
                        >
                          <ChevronLeftIcon class="size-5" />
                        </button>
                      </div>
                      <div class="pointer-events-auto">
                        <button
                          v-if="game.mImageCarouselObjectIds.length > 1"
                          @click.stop="nextImage()"
                          class="p-2 rounded-full bg-zinc-900/50 text-zinc-100 hover:bg-zinc-900/80 transition-all duration-300 hover:scale-110"
                        >
                          <ChevronRightIcon class="size-5" />
                        </button>
                      </div>
                    </div>

                    <div
                      class="absolute inset-0 bg-gradient-to-t from-black/50 to-transparent opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none"
                    />
                    <div
                      class="absolute bottom-4 right-4 flex items-center gap-x-2 text-white opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none"
                    >
                      <ArrowsPointingOutIcon class="size-5" />
                      <span class="text-sm font-medium">View Fullscreen</span>
                    </div>
                  </div>

                  <div
                    class="absolute -bottom-2 left-1/2 -translate-x-1/2 flex gap-x-2"
                  >
                    <button
                      v-for="(_, index) in game.mImageCarouselObjectIds"
                      :key="index"
                      @click.stop="currentImageIndex = index"
                      class="w-1.5 h-1.5 rounded-full transition-all"
                      :class="[
                        currentImageIndex === index
                          ? 'bg-zinc-100 scale-125'
                          : 'bg-zinc-600 hover:bg-zinc-500',
                      ]"
                    />
                  </div>
                </div>

                <div
                  v-else
                  class="aspect-video rounded-lg overflow-hidden bg-zinc-900/50 flex flex-col items-center justify-center text-center px-4"
                >
                  <PhotoIcon class="size-12 text-zinc-500 mb-2" />
                  <p class="text-zinc-400 font-medium">No images available</p>
                  <p class="text-zinc-500 text-sm">
                    Game screenshots will appear here when available
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>

  <EmulatorRepairModal
    v-if="repairOpen"
    v-model="repairOpen"
    :requirements="missingEmulators"
    @queued="refreshEmulators"
  />
  <EmulatorUninstallModal
    v-if="emulatorToUninstall && emulatorUninstallOpen"
    v-model="emulatorUninstallOpen"
    :emulator="emulatorToUninstall"
  />

  <ModalTemplate v-model="installFlowOpen">
    <template #default>
      <div class="sm:flex sm:items-start">
        <div class="mt-3 text-center sm:mt-0 sm:text-left">
          <h3 class="text-base font-semibold text-zinc-100">
            Install {{ game.mName }}?
          </h3>
          <div class="mt-2">
            <p class="text-sm text-zinc-400">
              Drop will add {{ game.mName }} to the queue to be downloaded.
              While downloading, Drop may use up a large amount of resources,
              particularly network bandwidth and CPU utilisation.
            </p>
          </div>
        </div>
      </div>

      <div class="space-y-6">
        <div v-if="buildOptions.length">
          <label
            for="install-build"
            class="block text-sm/6 font-medium text-zinc-100"
            >Build</label
          >
          <select
            id="install-build"
            v-model="installPlatform"
            class="mt-2 min-h-10 w-full rounded-md border-zinc-700 bg-zinc-800 text-sm text-zinc-100 focus:border-blue-600 focus:ring-blue-600"
            @change="installVersionIndex = -1"
          >
            <option
              v-if="
                !buildOptions.some(
                  (option) => option.platform === installPlatform,
                )
              "
              :value="installPlatform"
            >
              {{ installPlatform }} — Installed build (unavailable)
            </option>
            <option
              v-for="option in buildOptions"
              :key="option.platform"
              :value="option.platform"
            >
              {{ buildLabel(option) }}
            </option>
          </select>
          <p class="mt-2 text-xs text-zinc-400">
            Linux builds run natively. Windows builds marked for Proton run
            through Proton on Linux.
          </p>
        </div>
        <div v-if="versionOptions && versionOptions.length > 0">
          <Listbox as="div" v-model="installVersionIndex">
            <ListboxLabel class="block text-sm/6 font-medium text-zinc-100"
              >Version</ListboxLabel
            >
            <div class="relative mt-2">
              <ListboxButton
                class="relative w-full cursor-default rounded-md bg-zinc-800 py-1.5 pl-3 pr-10 text-left text-zinc-100 shadow-sm ring-1 ring-inset ring-zinc-700 focus:outline-none focus:ring-2 focus:ring-blue-600 sm:text-sm/6"
              >
                <span class="block truncate">{{
                  formatVersionOptionText(installVersionIndex)
                }}</span>
                <span
                  class="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2"
                >
                  <ChevronUpDownIcon
                    class="h-5 w-5 text-gray-400"
                    aria-hidden="true"
                  />
                </span>
              </ListboxButton>

              <div
                v-if="installVersionIndex == -1"
                class="mt-3 rounded-md bg-blue-500/10 p-2 outline outline-blue-500/20"
              >
                <div class="flex">
                  <div class="shrink-0">
                    <InformationCircleIcon
                      class="size-4 text-blue-400"
                      aria-hidden="true"
                    />
                  </div>
                  <div class="ml-2 flex-1 md:flex md:justify-between">
                    <p class="text-xs text-blue-300">
                      "Latest" notifies you about new versions of the selected
                      build. It will not switch between native Linux and
                      Windows/Proton. Choose a specific version to disable
                      update notifications.
                    </p>
                  </div>
                </div>
              </div>
              <div
                v-else
                class="mt-3 rounded-md bg-blue-500/10 p-2 outline outline-blue-500/20"
              >
                <div class="flex">
                  <div class="shrink-0">
                    <InformationCircleIcon
                      class="size-4 text-blue-400"
                      aria-hidden="true"
                    />
                  </div>
                  <div class="ml-2 flex-1 md:flex md:justify-between">
                    <p class="text-xs text-blue-300">
                      This game will be pinned to "{{
                        currentVersionOption?.displayName ||
                        currentVersionOption?.versionPath
                      }}"
                    </p>
                  </div>
                </div>
              </div>

              <transition
                leave-active-class="transition ease-in duration-100"
                leave-from-class="opacity-100"
                leave-to-class="opacity-0"
              >
                <ListboxOptions
                  class="absolute z-10 mt-1 max-h-60 w-full overflow-auto rounded-md bg-zinc-900 py-1 text-base shadow-lg ring-1 ring-black ring-opacity-5 focus:outline-none sm:text-sm"
                >
                  <ListboxOption
                    as="template"
                    :value="-1"
                    v-slot="{ active, selected }"
                  >
                    <li
                      :class="[
                        active ? 'bg-blue-600 text-white' : 'text-zinc-300',
                        'relative cursor-default select-none py-2 pl-3 pr-9',
                      ]"
                    >
                      <span
                        :class="[
                          selected
                            ? 'font-semibold text-zinc-100'
                            : 'font-normal',
                          'block truncate',
                        ]"
                        >{{ formatVersionOptionText(-1) }}</span
                      >

                      <span
                        v-if="selected"
                        :class="[
                          active ? 'text-white' : 'text-blue-600',
                          'absolute inset-y-0 right-0 flex items-center pr-4',
                        ]"
                      >
                        <CheckIcon class="h-5 w-5" aria-hidden="true" />
                      </span>
                    </li>
                  </ListboxOption>

                  <ListboxOption
                    as="template"
                    v-for="(version, versionIdx) in versionOptions"
                    :key="`${version.versionId}-${version.platform}`"
                    :value="versionIdx"
                    v-slot="{ active, selected }"
                  >
                    <li
                      :class="[
                        active ? 'bg-blue-600 text-white' : 'text-zinc-300',
                        'relative cursor-default select-none py-2 pl-3 pr-9',
                      ]"
                    >
                      <span
                        :class="[
                          selected
                            ? 'font-semibold text-zinc-100'
                            : 'font-normal',
                          'block truncate',
                        ]"
                        >{{ formatVersionOptionText(versionIdx) }}</span
                      >

                      <span
                        v-if="selected"
                        :class="[
                          active ? 'text-white' : 'text-blue-600',
                          'absolute inset-y-0 right-0 flex items-center pr-4',
                        ]"
                      >
                        <CheckIcon class="h-5 w-5" aria-hidden="true" />
                      </span>
                    </li>
                  </ListboxOption>
                </ListboxOptions>
              </transition>
            </div>
          </Listbox>
        </div>
        <div
          v-else-if="versionOptions === null || versionOptions?.length == 0"
          class="mt-1 rounded-md bg-red-600/10 p-4"
        >
          <div class="flex">
            <div class="flex-shrink-0">
              <XCircleIcon class="h-5 w-5 text-red-600" aria-hidden="true" />
            </div>
            <div class="ml-3">
              <h3 class="text-sm font-medium text-red-600">
                There are no supported versions to install. Please contact your
                server admin or try again later.
              </h3>
            </div>
          </div>
        </div>
        <div v-else class="w-full flex items-center justify-center p-4">
          <div role="status">
            <svg
              aria-hidden="true"
              class="w-7 h-7 text-transparent animate-spin fill-white"
              viewBox="0 0 100 101"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M100 50.5908C100 78.2051 77.6142 100.591 50 100.591C22.3858 100.591 0 78.2051 0 50.5908C0 22.9766 22.3858 0.59082 50 0.59082C77.6142 0.59082 100 22.9766 100 50.5908ZM9.08144 50.5908C9.08144 73.1895 27.4013 91.5094 50 91.5094C72.5987 91.5094 90.9186 73.1895 90.9186 50.5908C90.9186 27.9921 72.5987 9.67226 50 9.67226C27.4013 9.67226 9.08144 27.9921 9.08144 50.5908Z"
                fill="currentColor"
              />
              <path
                d="M93.9676 39.0409C96.393 38.4038 97.8624 35.9116 97.0079 33.5539C95.2932 28.8227 92.871 24.3692 89.8167 20.348C85.8452 15.1192 80.8826 10.7238 75.2124 7.41289C69.5422 4.10194 63.2754 1.94025 56.7698 1.05124C51.7666 0.367541 46.6976 0.446843 41.7345 1.27873C39.2613 1.69328 37.813 4.19778 38.4501 6.62326C39.0873 9.04874 41.5694 10.4717 44.0505 10.1071C47.8511 9.54855 51.7191 9.52689 55.5402 10.0491C60.8642 10.7766 65.9928 12.5457 70.6331 15.2552C75.2735 17.9648 79.3347 21.5619 82.5849 25.841C84.9175 28.9121 86.7997 32.2913 88.1811 35.8758C89.083 38.2158 91.5421 39.6781 93.9676 39.0409Z"
                fill="currentFill"
              />
            </svg>
            <span class="sr-only">Loading...</span>
          </div>
        </div>
        <div
          v-if="currentVersionOption?.protonSetupRequired"
          role="status"
          class="rounded-md bg-amber-500/10 p-3 text-sm text-amber-200"
        >
          <p class="font-semibold">Proton setup required</p>
          <p class="mt-1">
            You can download this Windows build now. Before playing, install
            umu-run and select a Proton version in
            <NuxtLink to="/settings/compat" class="underline underline-offset-2"
              >Settings → Compatibility</NuxtLink
            >.
          </p>
        </div>
        <div v-if="installDirs">
          <InstallDirectorySelector
            :install-dirs="installDirs"
            v-model="installDir"
            :label="
              game.type === 'Emulator' ? 'Emulator location' : 'Game location'
            "
            :default-index="mainLocations?.defaultIndex"
          />
          <InstallDirectorySelector
            v-if="emulatorsToDownload.length"
            class="mt-4"
            :install-dirs="installLocations?.emulator.directories ?? []"
            v-model="emulatorInstallDir"
            label="Emulator location"
            :default-index="installLocations?.emulator.defaultIndex"
          />
        </div>
        <div
          v-if="
            currentVersionOption?.requiredContent &&
            currentVersionOption.requiredContent.length > 0
          "
        >
          <div class="border-b border-white/10 py-2">
            <h3 class="text-sm font-semibold text-white">Required emulators</h3>
            <p class="mt-1 text-xs text-gray-400">
              Installed builds will be reused. Drop will download only missing
              emulator builds.
            </p>
          </div>
          <ul role="list" class="mt-2 divide-y divide-white/5">
            <li
              v-for="content in currentVersionOption.requiredContent"
              :key="content.versionId"
              class="rounded-lg relative flex justify-between bg-zinc-950 ring-2 ring-zinc-800 px-2 py-3"
            >
              <div class="flex min-w-0 gap-x-2">
                <img
                  class="size-12 flex-none"
                  :src="useObject(content.iconObjectId)"
                  alt=""
                />
                <div class="min-w-0 flex-auto">
                  <p class="text-sm/6 font-semibold text-white">
                    {{ content.name }}
                    <span
                      v-if="content.versionName"
                      class="ml-2 font-mono text-blue-200"
                      >{{ content.versionName }}</span
                    >
                  </p>
                  <p
                    v-if="content.installedPath"
                    class="mt-2 whitespace-normal break-all text-xs text-green-300"
                  >
                    Already installed · {{ content.installedPath }}
                  </p>
                  <p v-else class="mt-2 text-xs text-blue-200">
                    Download required
                  </p>
                  <p class="mt-1 flex text-xs/5 text-gray-400">
                    {{ content.shortDescription }}
                  </p>
                </div>
              </div>
              <div class="flex shrink-0 items-center gap-x-2">
                <div class="hidden sm:flex sm:flex-col sm:items-end">
                  <p
                    class="inline-flex items-center gap-x-1 text-xs/5 text-gray-400"
                  >
                    {{ formatKilobytes(content.size.installSize / 1024) }}B
                    <ServerIcon class="size-3" />
                  </p>
                </div>
                <CheckIcon
                  v-if="content.installedPath"
                  class="size-5 flex-none text-green-500"
                  aria-hidden="true"
                />
                <ArrowDownTrayIcon
                  v-else
                  class="size-5 flex-none text-blue-300"
                  aria-hidden="true"
                />
              </div>
            </li>
          </ul>
        </div>
      </div>

      <div v-if="installError" class="mt-1 rounded-md bg-red-600/10 p-4">
        <div class="flex">
          <div class="flex-shrink-0">
            <XCircleIcon class="h-5 w-5 text-red-600" aria-hidden="true" />
          </div>
          <div class="ml-3">
            <h3 class="text-sm font-medium text-red-600">
              {{ installError }}
            </h3>
          </div>
        </div>
      </div>
    </template>
    <template #buttons>
      <LoadingButton
        @click="() => install()"
        :disabled="!(versionOptions && versionOptions.length > 0)"
        :loading="installLoading"
        type="submit"
        class="ml-2 w-full sm:w-fit"
      >
        Install
      </LoadingButton>
      <button
        type="button"
        class="mt-3 inline-flex w-full justify-center rounded-md bg-zinc-800 px-3 py-2 text-sm font-semibold text-zinc-100 shadow-sm ring-1 ring-inset ring-zinc-700 hover:bg-zinc-900 sm:mt-0 sm:w-auto"
        @click="installFlowOpen = false"
        ref="cancelButtonRef"
      >
        Cancel
      </button>
    </template>
  </ModalTemplate>

  <ModalTemplate :model-value="launchOptionsOpen">
    <template #default>
      <div class="sm:flex sm:items-start">
        <div class="mt-3 text-center sm:mt-0 sm:text-left">
          <h3 class="text-base font-semibold text-zinc-100">
            Launch {{ game.mName }}
          </h3>
          <div class="mt-2">
            <p class="text-sm text-zinc-400">
              The instance admin has configured multiple ways to start this
              game. Select an option to start.
            </p>
          </div>
        </div>
      </div>

      <ol class="space-y-2">
        <li v-for="(launchData, launchIdx) in launchOptions!">
          <button
            class="transition w-full rounded-sm bg-zinc-800 inline-flex items-center text-sm py-2 px-3 gap-x-2 text-zinc-100 hover:text-zinc-300 hover:bg-zinc-700"
            @click="() => launchIndex(launchIdx)"
          >
            <PlayIcon class="size-4" />
            <span>
              {{ launchData.name }}
            </span>
          </button>
        </li>
      </ol>
    </template>
    <template #buttons>
      <button
        type="button"
        class="mt-3 inline-flex w-full justify-center rounded-md bg-zinc-800 px-3 py-2 text-sm font-semibold text-zinc-100 shadow-sm ring-1 ring-inset ring-zinc-700 hover:bg-zinc-900 sm:mt-0 sm:w-auto"
        @click="launchOptions = undefined"
        ref="cancelButtonRef"
      >
        Cancel
      </button>
    </template>
  </ModalTemplate>

  <!-- 
  Dear future DecDuck,
  This v-if is necessary for Vue rendering reasons
  (it tries to access the game version for not installed games)
  You have already tried to remove it
  Don't.  
  -->
  <GameOptionsModal
    v-if="
      status.type === 'Installed' &&
      (status.install_type.type == InstalledType.Installed ||
        status.install_type.type == InstalledType.SetupRequired)
    "
    v-model="configureModalOpen"
    :game-id="game.id"
  />

  <Transition
    enter="transition ease-out duration-300"
    enter-from="opacity-0"
    enter-to="opacity-100"
    leave="transition ease-in duration-200"
    leave-from="opacity-100"
    leave-to="opacity-0"
  >
    <div
      v-if="fullscreenImage"
      class="fixed inset-0 z-50 bg-black/95 flex items-center justify-center"
      @click="fullscreenImage = null"
    >
      <div
        class="relative w-full h-full flex items-center justify-center"
        @click.stop
      >
        <button
          class="absolute top-4 right-4 p-2 rounded-full bg-zinc-900/50 text-zinc-100 hover:bg-zinc-900 transition-colors"
          @click.stop="fullscreenImage = null"
        >
          <XMarkIcon class="size-6" />
        </button>

        <button
          v-if="game.mImageCarouselObjectIds.length > 1"
          @click.stop="previousImage()"
          class="absolute left-4 p-3 rounded-full bg-zinc-900/50 text-zinc-100 hover:bg-zinc-900 transition-colors"
        >
          <ChevronLeftIcon class="size-6" />
        </button>
        <button
          v-if="game.mImageCarouselObjectIds.length > 1"
          @click.stop="nextImage()"
          class="absolute right-4 p-3 rounded-full bg-zinc-900/50 text-zinc-100 hover:bg-zinc-900 transition-colors"
        >
          <ChevronRightIcon class="size-6" />
        </button>

        <TransitionGroup
          name="slide"
          tag="div"
          class="w-full h-full flex items-center justify-center"
          @click.stop
        >
          <img
            v-for="(url, index) in game.mImageCarouselObjectIds"
            v-show="currentImageIndex === index"
            :key="index"
            :src="useObject(url)"
            class="max-h-[90vh] max-w-[90vw] object-contain"
            :alt="`${game.mName} screenshot ${index + 1}`"
          />
        </TransitionGroup>

        <div
          class="absolute bottom-4 left-1/2 -translate-x-1/2 px-4 py-2 rounded-full bg-zinc-900/50 backdrop-blur-sm"
        >
          <p class="text-zinc-100 text-sm font-medium">
            {{ currentImageIndex + 1 }} /
            {{ game.mImageCarouselObjectIds.length }}
          </p>
        </div>
      </div>
    </div>
  </Transition>

  <DependencyRequiredModal
    v-if="dependencyRequiredModal"
    v-model="dependencyRequiredModal"
  />
</template>

<script setup lang="ts">
import { requestPreferredSteamLaunch } from "~/utils/steam-launch";
import {
  Listbox,
  ListboxButton,
  ListboxLabel,
  ListboxOption,
  ListboxOptions,
} from "@headlessui/vue";
import {
  CheckIcon,
  ChevronUpDownIcon,
  WrenchIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  XMarkIcon,
  ArrowsPointingOutIcon,
  PhotoIcon,
  PlayIcon,
  InformationCircleIcon,
} from "@heroicons/vue/20/solid";
import { BuildingStorefrontIcon } from "@heroicons/vue/24/outline";
import {
  ArrowDownTrayIcon,
  CheckCircleIcon,
  MapPinIcon,
  ServerIcon,
  XCircleIcon,
} from "@heroicons/vue/24/solid";
import { invoke } from "@tauri-apps/api/core";
import {
  InstalledType,
  type InstalledEmulator,
  type InstallLocations,
} from "~/types";
import { renderGameDescription } from "~/utils/game-description";
import { canStartStoreDownload } from "~/utils/store-download";
import {
  gameInstallPlan,
  emulatorRequirementsChanged,
} from "~/utils/emulator-install";
import {
  defaultInstallIndex,
  installPlanLocations,
} from "~/utils/install-locations";
import {
  availableBuilds,
  buildLabel,
  versionsForBuild,
} from "~/utils/version-options";

const route = useRoute();
const router = useRouter();
const id = route.params.id.toString();

const { game, status, version } = await useGame(id);
const {
  missing: missingEmulators,
  loading: checkingDependencies,
  error: dependencyCheckError,
  refresh: refreshEmulators,
} = await useMissingEmulators(id);
const repairOpen = ref(false);
const emulatorToUninstall = ref<InstalledEmulator>();
const emulatorUninstallOpen = ref(false);
const downloadQueue = useQueueState();
const dependencyDownloadActive = computed(() =>
  missingEmulators.value.some((dependency) =>
    downloadQueue.value.queue.some(
      (item) =>
        item.meta.id === dependency.gameId &&
        item.meta.version === dependency.versionId &&
        item.meta.targetPlatform === dependency.platform,
    ),
  ),
);

async function repairEmulators() {
  await refreshEmulators();
  if (missingEmulators.value.length && !dependencyCheckError.value)
    repairOpen.value = true;
}

const bannerUrl = await useObject(game.mBannerObjectId);

const htmlDescription = renderGameDescription(game.mDescription, useObject);

function formatPlaytime(seconds: number): string {
  return `${(seconds / 3600).toFixed(1)}h`;
}

const installFlowOpen = ref(false);
const allVersionOptions = ref<undefined | Array<VersionOption>>();
const installPlatform = ref("");
const buildOptions = computed(() =>
  availableBuilds(allVersionOptions.value ?? []),
);
const versionOptions = computed(
  () =>
    allVersionOptions.value &&
    versionsForBuild(allVersionOptions.value, installPlatform.value),
);
const installDirs = ref<undefined | Array<string>>();
const installLocations = ref<InstallLocations>();
const mainLocations = computed(
  () =>
    installLocations.value?.[game.type === "Emulator" ? "emulator" : "game"],
);
const currentImageIndex = ref(0);

const configureModalOpen = ref(false);

async function installFlow() {
  installFlowOpen.value = true;
  allVersionOptions.value = undefined;
  installDirs.value = undefined;
  installLocations.value = undefined;
  installError.value = undefined;

  try {
    allVersionOptions.value = await invoke("fetch_game_version_options", {
      gameId: game.id,
    });
    const first = allVersionOptions.value?.[0];
    installPlatform.value = first?.installedPlatform ?? first?.platform ?? "";
    installVersionIndex.value = -1;
    installLocations.value = await fetchInstallLocations();
    const main = mainLocations.value!;
    installDirs.value = main.directories;
    installDir.value = defaultInstallIndex(main);
    emulatorInstallDir.value = defaultInstallIndex(
      installLocations.value.emulator,
    );
  } catch (error) {
    installError.value = (error as string).toString();
    allVersionOptions.value = undefined;
  }
}

const installLoading = ref(false);
const installError = ref<string | undefined>();
const installVersionIndex = ref(-1);
const installDir = ref(0);
const emulatorInstallDir = ref(0);

onMounted(async () => {
  if (route.query.download !== "1") return;

  const { download: _download, ...query } = route.query;
  await router.replace({ path: route.path, query });
  if (status?.value.type !== "Remote") return;

  await installFlow();
  if (!installFlowOpen.value) return;
  if (
    canStartStoreDownload(
      status?.value,
      versionOptions.value,
      installDirs.value,
      mainLocations.value?.defaultIndex,
    )
  ) {
    installVersionIndex.value = -1;
    await install();
  }
});

async function install() {
  try {
    if (installLoading.value) return;
    if (!versionOptions.value?.length)
      throw new Error("No compatible versions are available");
    if (!installDirs.value?.[installDir.value])
      throw new Error("Select an install folder");
    installLoading.value = true;
    const selectedOption =
      versionOptions.value[Math.max(installVersionIndex.value, 0)];
    if (!selectedOption) throw new Error("Select a version to install");
    // Recheck local readiness and current server pins before skipping dependencies.
    const refreshed = await invoke<VersionOption[]>(
      "fetch_game_version_options",
      { gameId: game.id },
    );
    const versionOption = refreshed.find(
      (option) =>
        option.versionId === selectedOption.versionId &&
        option.platform === selectedOption.platform,
    );
    if (!versionOption)
      throw new Error(
        "The selected build is no longer available. Reopen the install dialog.",
      );
    allVersionOptions.value = refreshed;
    const refreshedBuild = versionsForBuild(refreshed, installPlatform.value);
    if (
      installVersionIndex.value === -1 &&
      refreshedBuild[0]?.versionId !== selectedOption.versionId
    )
      throw new Error(
        "The latest game version changed. Review the selected version, then click Install again.",
      );
    if (installVersionIndex.value >= 0)
      installVersionIndex.value = refreshedBuild.findIndex(
        (option) => option.versionId === selectedOption.versionId,
      );
    if (
      emulatorRequirementsChanged(
        selectedOption.requiredContent,
        versionOption.requiredContent,
      )
    )
      throw new Error(
        "The emulator requirements changed. Review the locations and requirements, then click Install again.",
      );
    const isLatest = installVersionIndex.value == -1;

    const plan = gameInstallPlan(
      { gameId: game.id, versionId: versionOption.versionId },
      versionOption.requiredContent,
      isLatest,
    );

    const games = installPlanLocations(
      plan,
      game.id,
      installDirs.value[installDir.value],
      installLocations.value?.emulator.directories[emulatorInstallDir.value],
    );

    for (const game of games) {
      await invoke("download_game", {
        gameId: game.gameId,
        versionId: game.versionId,
        installPath: game.installPath,
        targetPlatform: versionOption.platform,
        enableUpdates: game.enableUpdates,
        reuseInstalled: game.gameId !== id,
        requiredLaunchIds: versionOption.requiredContent.find(
          (dependency) =>
            dependency.gameId === game.gameId &&
            dependency.versionId === game.versionId,
        )?.launchIds,
      });
    }

    installFlowOpen.value = false;
  } catch (error) {
    installError.value = (error as string).toString();
  }

  installLoading.value = false;
}

const currentVersionOption = computed(
  () => versionOptions.value?.[Math.max(installVersionIndex.value, 0)],
);
const emulatorsToDownload = computed(
  () =>
    currentVersionOption.value?.requiredContent.filter(
      (dependency) => !dependency.installedPath,
    ) ?? [],
);

function formatVersionOptionText(index: number) {
  if (!versionOptions.value) return undefined;
  const versionOption = versionOptions.value[Math.max(index, 0)];
  if (!versionOption) return undefined;
  const template = `${versionOption.displayName || versionOption.versionPath} · ${buildLabel(versionOption)}, ${formatKilobytes(versionOption.size.installSize / 1024)}B`;
  if (index == -1) {
    return `Latest (${template})`;
  }
  return template;
}

async function resumeDownload() {
  try {
    await invoke("resume_download", { gameId: game.id });
  } catch (e) {
    console.error(e);
  }
}

const launchOptions = ref<Array<{ name: string }> | undefined>(undefined);
const launchOptionsOpen = computed(() => launchOptions.value !== undefined);

const steamActions = ref<{ beforeUninstall: () => Promise<boolean> }>();
async function launch(direct = false) {
  if (checkingDependencies.value) return;
  await refreshEmulators();
  if (missingEmulators.value.length || dependencyCheckError.value) {
    await repairEmulators();
    return;
  }
  if (
    !direct &&
    status.value.type === "Installed" &&
    status.value.install_type.type === InstalledType.Installed
  ) {
    try {
      if (
        await requestPreferredSteamLaunch(game.id, direct, (id) =>
          invoke<boolean>("steam_launch_preferred", { id }),
        )
      ) {
        createModal(
          ModalType.Notification,
          {
            title: "Launch requested in Steam",
            description:
              "The request was handed to Steam. Drop cannot confirm launch, track playtime, or stop this game.",
            buttonText: "Close",
          },
          (e, c) => c(),
        );
        return;
      }
    } catch (e) {
      createModal(
        ModalType.Notification,
        {
          title: "Steam launch unavailable",
          description:
            String(e) + " Use Play directly with Drop to launch without Steam.",
          buttonText: "Close",
        },
        (e, c) => c(),
      );
      return;
    }
  }
  if (
    status.value.type == "Installed" &&
    status.value.install_type.type == InstalledType.SetupRequired
  ) {
    await launchIndex(0);
    return;
  }
  try {
    const fetchedLaunchOptions = await invoke<Array<{ name: string }>>(
      "get_launch_options",
      { id: game.id },
    );
    if (fetchedLaunchOptions.length == 1) {
      await launchIndex(0);
      return;
    }
    launchOptions.value = fetchedLaunchOptions;
  } catch (e) {
    createModal(
      ModalType.Notification,
      {
        title: `Couldn't run "${game.mName}"`,
        description: `Drop failed to launch "${game.mName}": ${e}`,
        buttonText: "Close",
      },
      (e, c) => c(),
    );
    console.error(e);
  }
}

const dependencyRequiredModal = ref<
  { gameId: string; versionId: string; platform: string } | undefined
>(undefined);

async function launchIndex(index: number) {
  launchOptions.value = undefined;
  try {
    const result = await invoke<LaunchResult>("launch_game", {
      id: game.id,
      index,
    });
    if (result.result == "InstallRequired") {
      dependencyRequiredModal.value = {
        gameId: result.data[0],
        versionId: result.data[1],
        platform: result.data[2],
      };
    }
  } catch (e) {
    createModal(
      ModalType.Notification,
      {
        title: `Couldn't run "${game.mName}"`,
        description: `Drop failed to launch "${game.mName}": ${e}`,
        buttonText: "Close",
      },
      (e, c) => c(),
    );
  }
}

async function queue() {
  router.push("/queue");
}

async function uninstall() {
  if (game.type === "Emulator") {
    try {
      const emulators = await invoke<InstalledEmulator[]>(
        "fetch_installed_emulators",
      );
      emulatorToUninstall.value = emulators.find(
        (emulator) => emulator.gameId === game.id,
      );
      if (!emulatorToUninstall.value)
        throw new Error(
          "The installed emulator could not be found. Refresh the Emulators page and try again.",
        );
      emulatorUninstallOpen.value = true;
    } catch (cause) {
      createModal(
        ModalType.Notification,
        {
          title: "Could not uninstall emulator",
          description: String(cause),
          buttonText: "Close",
        },
        (e, c) => c(),
      );
    }
    return;
  }
  if (
    steamActions.value?.beforeUninstall &&
    !(await steamActions.value.beforeUninstall())
  )
    return;
  await invoke("uninstall_game", { gameId: game.id });
}

async function openGameFolder() {
  const gameStatus = status?.value;
  if (gameStatus?.type !== "Installed") return;

  try {
    await invoke("open_fs", { path: gameStatus.install_dir });
  } catch (e) {
    createModal(
      ModalType.Notification,
      {
        title: "Couldn't open game folder",
        description: `Drop failed to open the folder for "${game.mName}": ${e}`,
        buttonText: "Close",
      },
      (e, c) => c(),
    );
    console.error(e);
  }
}

async function kill() {
  try {
    await invoke("kill_game", { gameId: game.id });
  } catch (e) {
    createModal(
      ModalType.Notification,
      {
        title: `Couldn't stop "${game.mName}"`,
        description: `Drop failed to stop "${game.mName}": ${e}`,
        buttonText: "Close",
      },
      (e, c) => c(),
    );
    console.error(e);
  }
}

function nextImage() {
  currentImageIndex.value =
    (currentImageIndex.value + 1) % game.mImageCarouselObjectIds.length;
}

function previousImage() {
  currentImageIndex.value =
    (currentImageIndex.value - 1 + game.mImageCarouselObjectIds.length) %
    game.mImageCarouselObjectIds.length;
}

const fullscreenImage = ref<string | null>(null);
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.slide-enter-active,
.slide-leave-active {
  transition: all 0.3s ease;
  position: absolute;
}

.slide-enter-from {
  opacity: 0;
  transform: translateX(100%);
}

.slide-leave-to {
  opacity: 0;
  transform: translateX(-100%);
}

.custom-scrollbar {
  scrollbar-width: thin;
  scrollbar-color: rgb(82 82 91) transparent;
}

.custom-scrollbar::-webkit-scrollbar {
  width: 6px;
}

.custom-scrollbar::-webkit-scrollbar-track {
  background: transparent;
}

.custom-scrollbar::-webkit-scrollbar-thumb {
  background-color: rgb(82 82 91);
  border-radius: 3px;
}
</style>
