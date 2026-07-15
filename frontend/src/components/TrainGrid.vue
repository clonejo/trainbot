<script setup lang="ts">
import type { Train as TrainType } from '@/lib/db'
import { getBlobThumbURL, getBlobURL, imgFileName, mp4FileName } from '@/lib/paths'
import RelativeTime from '@/components/RelativeTime.vue'
import FavoriteIcon from '@/components/FavoriteIcon.vue'

defineProps<{
  trains: TrainType[]
}>()
</script>

<template>
  <v-container fluid>
    <v-row dense>
      <v-col v-for="train in trains" v-bind:key="train.id" cols="6" sm="3" md="2" xl="1">
        <router-link
          :to="{ name: 'trainDetail', params: { id: train.id } }"
          style="text-decoration: none; color: inherit"
        >
          <v-card>
            <div class="video-thumb">
              <video
                :src="getBlobURL(mp4FileName(train.start_ts))"
                :poster="getBlobThumbURL(imgFileName(train.start_ts))"
                class="video-thumb-el"
                autoplay
                muted
                loop
                playsinline
              ></video>
              <div class="video-thumb-gradient"></div>
              <v-card-title class="text-white video-thumb-title">
                <RelativeTime :ts="train.start_ts" />
                <FavoriteIcon :id="train.id" />
              </v-card-title>
            </div>
          </v-card>
        </router-link>
      </v-col>
    </v-row>
  </v-container>
</template>

<style scoped>
.pointer {
  cursor: pointer;
}

.video-thumb {
  position: relative;
  height: 200px;
  overflow: hidden;
}

.video-thumb-el {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

.video-thumb-gradient {
  position: absolute;
  inset: 0;
  background: linear-gradient(to bottom, rgba(0, 0, 0, 0.1), rgba(0, 0, 0, 0.5));
  pointer-events: none;
}

.video-thumb-title {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
}
</style>
